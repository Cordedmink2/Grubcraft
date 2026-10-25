//! Soldier models on the GPU: every model's vertices and indices in one
//! buffer each, the bone animation texture, the material texture array, and
//! one instanced draw per model and level of detail.
//!
//! Soldiers are skinned once a frame. A compute pass poses every vertex of
//! every soldier the frame draws (`cs_skin` in `shaders/world.wgsl`) and
//! keeps its world position, normal and tangent; the shadow map and the
//! world pass then read those instead of each skinning the model again.

use std::mem::offset_of;

use crate::detail::{DetailGpu, upload_bc7_layers};
use bytemuck::{Pod, Zeroable};
use gc_assets::anim::{JointRecord, Role};
use gc_assets::model::{
    BONE_TEXELS_PER_ROW, LAYERS_PER_MATERIAL, LODS, ModelVertex, Models, TEXTURE_MIPS, TEXTURE_SIZE,
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
/// Threads in a workgroup of `cs_skin` (`cs_pose` has one per bone).
const SKIN_GROUP: u32 = 128;
/// Runs of soldiers one skinning pass can be told of (`Jobs` in world.wgsl):
/// one per model and level of detail, buildings included.
pub(crate) const MAX_JOBS: usize = 1023;
/// Storage buffers the posing and skinning passes bind: the model vertices
/// (two halves), the jobs, the skinned positions and shading, the instances,
/// the palette and the anim tables.
pub(crate) const POSE_STORAGE_BUFFERS: u32 = 8;
/// Bytes of one job (`Job` in world.wgsl): the run, then where its pose goes.
const JOB_BYTES: usize = 32;
// One dispatch must cover every vertex, whatever the GPU.
const _: () = assert!(SKIN_VERTICES.div_ceil(SKIN_GROUP as usize) <= 65_535);
/// Bone matrices the pose pass can build in a frame, all soldiers together:
/// 48 bytes each, 48 MB in all, though the palette only grows as far as
/// frames need. Soldiers past it are left out like those past
/// `SKIN_VERTICES`.
pub const PALETTE_BONES: usize = 1 << 20;
/// Bone matrices the palette starts with.
const PALETTE_START: usize = 64 << 10;
/// Bytes of one bone matrix in the palette (three rows of four floats).
const PALETTE_BONE_BYTES: usize = 48;
/// Soldiers posed along the pose dispatch's first dimension (`POSE_ROW` in
/// world.wgsl); a workgroup poses one soldier.
const POSE_ROW: u32 = 256;
const _: () = assert!((MAX_SOLDIER_INSTANCES as u32).div_ceil(POSE_ROW) <= 65_535);
// A workgroup has a thread per bone (`POSE_GROUP` in world.wgsl).
const _: () = assert!(gc_assets::model::MAX_BONES < 64);

/// The model attributes the world pass still reads, skinning having done
/// the rest: the team weight and material, the last two bytes of the
/// tangent (the second is the bitangent's handedness), colour and texture
/// coordinate.
const MODEL_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
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
    // The normal's last two bytes: its z, then the surface detail classes.
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint8x2,
        offset: offset_of!(ModelVertex, normal) as u64 + 2,
        shader_location: 6,
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

/// The coarsest level of detail whose soldiers get a contact shadow.
const CONTACT_MAX_LOD: usize = 1;

/// What the contact shadow reads of an instance: where it stands, how high
/// it is lifted and how big it is.
const CONTACT_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: offset_of!(SoldierInstance, pos) as u64,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: offset_of!(SoldierInstance, lift) as u64,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: offset_of!(SoldierInstance, scale) as u64,
        shader_location: 2,
    },
];

/// The tone byte (the last of [`SoldierInstance::color`]) that leaves a
/// soldier as painted.
pub const TONE_NEUTRAL: u8 = 128;

/// One layer of a soldier's pose on top of its base move: a move's two frame
/// rows to blend between and how far, how much of the layer shows (0 is
/// none), the [`Mask`](gc_assets::anim::Mask) of joints it moves, and
/// [`LAYER_ADDITIVE`] for a move stored as changes (a flinch). Models posed
/// from bone rows only play whole-body, plain layers. 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct PoseLayer {
    pub frames: [u16; 2],
    pub blend: u8,
    pub weight: u8,
    pub mask: u8,
    pub flags: u8,
}

/// [`PoseLayer::flags`]: the layer adds its change to the pose below.
pub const LAYER_ADDITIVE: u8 = 1;
/// Layers a soldier may play on top of its base move.
pub const POSE_LAYERS: usize = 3;

/// One soldier to draw. 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct SoldierInstance {
    /// Feet on the map, in tiles.
    pub pos: [f32; 2],
    /// Unit vector the soldier faces.
    pub facing: [f32; 2],
    /// Rows of the two frames of the base move to blend between: bone
    /// texture rows, or joint pose rows for a model fitted to a skeleton
    /// (as its clips name them).
    pub frames: [u16; 2],
    /// Blend toward the second frame and selection highlight, as 0 to 255;
    /// then how far the model is tipped over about its base, 0 (upright) to
    /// 255 (lying down, as a soldier without a death clip lies on his back),
    /// and the way it falls in its own space, as a turn from its front in
    /// 256ths (0 onto its back, 64 to its left; as scenery's `fall` reads
    /// it). A building toppling as it falls takes both.
    pub flags: [u8; 4],
    /// Team colour (rgb), and a per-soldier tone in the last byte: 128 leaves
    /// the model as painted, lower is darker and cooler, higher is lighter and
    /// warmer. It varies the cloth, skin and leather by about 8% and the team
    /// colour by a third of that.
    pub color: [u8; 4],
    /// Height above the ground, in tiles (negative sinks the fallen).
    pub lift: f32,
    /// Size relative to the model's baked height.
    pub scale: f32,
    /// Moves played on top of the base one, in order.
    pub layers: [PoseLayer; POSE_LAYERS],
    /// Where the soldier looks, turning chest, neck and head: yaw (127 is a
    /// quarter turn towards his left, the model's +x) and pitch (127 is an
    /// eighth of a turn down), for models fitted to a skeleton.
    pub look: [i8; 2],
    /// How much of `look` shows, 0 (none) to 255.
    pub look_weight: u8,
    /// How much the feet stay where the move puts them, 0 to
    /// [`FEET_AS_POSED`]: at 0 a fitted model's legs bend to stand on
    /// uneven ground; the game fades it up as a soldier falls.
    pub feet: u8,
}

/// The tip and way bytes of [`SoldierInstance::flags`] (its third and
/// fourth) for a model pitched `radians` about its base, forward end up when
/// positive: a cart on a slope. It is the tip the instance already has (a
/// fallen man or a toppling building, `tipped` in world.wgsl) taken toward
/// the model's back for a climb and its front for a descent, so lighting,
/// shadows and foot IK (off for a tipped model) already follow it. The angle
/// is kept under the quarter turn that means lying down.
pub fn pitch_bytes(radians: f32) -> [u8; 2] {
    let tip = (radians.abs() / std::f32::consts::FRAC_PI_2 * 255.0)
        .round()
        .min(254.0) as u8;
    [tip, if radians > 0.0 { 128 } else { 0 }]
}

/// [`SoldierInstance::feet`]: no foot IK at all (the fallen and the dead).
pub const FEET_AS_POSED: u8 = 255;

const _: () = assert!(size_of::<SoldierInstance>() == 64);

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
    /// The level of detail asked for (the contact shadows follow it).
    asked: usize,
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
    /// Every drawn soldier's bone matrices, written by the pose pass and
    /// read by the skinning pass; `palette_capacity` bones.
    palette: wgpu::Buffer,
    palette_capacity: usize,
    /// What the bind groups hold besides buffers.
    bones: wgpu::TextureView,
    srgb: wgpu::TextureView,
    linear: wgpu::TextureView,
    sampler: wgpu::Sampler,
    detail: DetailGpu,
    /// For the shadow and world passes.
    pub bind_group: wgpu::BindGroup,
    /// For the skinning pass.
    pub skin_group: wgpu::BindGroup,
    /// Per model, what the planner needs of it.
    pub levels: Vec<ModelInfo>,
    /// Skeletons, joint poses and fixes (see [`anim_gpu`]).
    anim: wgpu::Buffer,
    /// Everything but the skinned buffers.
    fixed_bytes: u64,
    /// Vertices in the skinning pass's first vertex binding (see
    /// [`vertex_split`]).
    split: u32,
}

/// What the planner needs of a model.
#[derive(Clone, Copy, Debug)]
pub struct ModelInfo {
    pub levels: [Level; LODS],
    /// Drawn with the textured pipeline.
    pub textured: bool,
    pub bones: u32,
    /// Where its fit starts in the anim buffer, 0 for a model posed from
    /// bone rows.
    pub fit: u32,
}

impl ModelGpu {
    pub fn gpu_bytes(&self) -> u64 {
        self.fixed_bytes
            + self.detail.gpu_bytes()
            + self.skin_pos.size()
            + self.skin_shade.size()
            + self.palette.size()
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            vertices: &self.vertices,
            split: self.split,
            origins: &self.origins,
            jobs: &self.jobs,
            skin_pos: &self.skin_pos,
            skin_shade: &self.skin_shade,
            palette: &self.palette,
            anim: &self.anim,
            bones: &self.bones,
            srgb: &self.srgb,
            linear: &self.linear,
            sampler: &self.sampler,
            detail: &self.detail.view,
            detail_sampler: &self.detail.sampler,
        }
    }

    /// Hold at least `vertices` skinned vertices and `bones` palette bones,
    /// each rounded up to a power of two (`SKIN_VERTICES` and
    /// `PALETTE_BONES` at most). The buffers are written afresh every frame,
    /// so nothing is copied.
    fn grow(
        &mut self,
        device: &wgpu::Device,
        soldiers: &SoldiersGpu,
        vertices: usize,
        bones: usize,
    ) {
        let skin = vertices > self.skin_capacity;
        let palette = bones > self.palette_capacity;
        if skin {
            self.skin_capacity = vertices.next_power_of_two().min(SKIN_VERTICES);
            (self.skin_pos, self.skin_shade) = skin_buffers(device, self.skin_capacity);
        }
        if palette {
            self.palette_capacity = bones.next_power_of_two().min(PALETTE_BONES);
            self.palette = palette_buffer(device, self.palette_capacity);
        }
        if skin || palette {
            (self.bind_group, self.skin_group) = bind_groups(device, soldiers, &self.parts());
        }
    }
}

/// The pose pass's output, for `capacity` bone matrices.
fn palette_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bone palette"),
        size: (capacity * PALETTE_BONE_BYTES) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    })
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

/// How many model vertices the skinning pass's first vertex binding holds:
/// all of them when the GPU's storage binding limit allows (most do), else
/// as many as fit, the rest going to the second binding (`set_vertices_hi`
/// in world.wgsl). The split is a multiple of the offset alignment.
pub(crate) fn vertex_split(device: &wgpu::Device, vertices: u64) -> u64 {
    let limits = device.limits();
    let step = (u64::from(limits.min_storage_buffer_offset_alignment) / VERTEX_BYTES).max(1);
    let most = limits.max_storage_buffer_binding_size / VERTEX_BYTES / step * step;
    vertices.min(most)
}

/// Bytes in one stored model vertex (`SetVertex` in world.wgsl).
pub(crate) const VERTEX_BYTES: u64 = size_of::<ModelVertex>() as u64;
// A split at a multiple of the offset alignment's worth of vertices is
// aligned only while the vertex size divides that (power of two) alignment.
const _: () = assert!(VERTEX_BYTES.is_power_of_two());

/// The two vertex bindings of the skinning pass, split after `split`
/// vertices (see [`vertex_split`]).
fn vertex_entries(vertices: &wgpu::Buffer, split: u32) -> [wgpu::BindGroupEntry<'_>; 2] {
    let all = vertices.size() / VERTEX_BYTES;
    let split = u64::from(split);
    let range = |binding, from: u64, to: u64| wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer: vertices,
            offset: from * VERTEX_BYTES,
            size: wgpu::BufferSize::new((to - from) * VERTEX_BYTES),
        }),
    };
    if all == 0 || split == all {
        // Everything in the first binding; the second, never read, repeats it.
        return [entry(9, vertices), entry(14, vertices)];
    }
    [range(9, 0, split), range(14, split, all)]
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
    split: u32,
    origins: &'a wgpu::Buffer,
    jobs: &'a wgpu::Buffer,
    skin_pos: &'a wgpu::Buffer,
    skin_shade: &'a wgpu::Buffer,
    palette: &'a wgpu::Buffer,
    anim: &'a wgpu::Buffer,
    bones: &'a wgpu::TextureView,
    srgb: &'a wgpu::TextureView,
    linear: &'a wgpu::TextureView,
    sampler: &'a wgpu::Sampler,
    detail: &'a wgpu::TextureView,
    detail_sampler: &'a wgpu::Sampler,
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
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(m.detail),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(m.detail_sampler),
            },
        ],
    });
    let [lo, hi] = vertex_entries(m.vertices, m.split);
    let skin = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("soldier skinning"),
        layout: &soldiers.skin_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(m.bones),
            },
            lo,
            hi,
            entry(10, &soldiers.instances),
            entry(11, m.jobs),
            entry(12, m.skin_pos),
            entry(13, m.skin_shade),
            entry(15, m.palette),
            entry(16, m.anim),
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
    /// A soft dark patch on the ground under every soldier.
    contact_pipeline: wgpu::RenderPipeline,
    /// Builds every soldier's bone matrices, then poses its vertices with
    /// them, before the passes that draw them.
    pose_pipeline: wgpu::ComputePipeline,
    skin_pipeline: wgpu::ComputePipeline,
    pub models_layout: wgpu::BindGroupLayout,
    pub skin_layout: wgpu::BindGroupLayout,
    pub instances: wgpu::Buffer,
    pub models: Option<ModelGpu>,
    /// This frame's runs of soldiers, from `prepare`.
    draws: Vec<Draw>,
    /// Scratch for what `prepare` uploads.
    jobs: Vec<[u32; 8]>,
    origins: Vec<u32>,
    /// Vertices the skinning pass poses this frame.
    threads: u32,
    /// Soldiers the pose pass poses this frame (a workgroup each).
    posed: u32,
}

pub(crate) fn make(
    device: &wgpu::Device,
    world_layout: &wgpu::BindGroupLayout,
    world_pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    hdr: crate::Hdr,
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
            // The shared surface detail array and its repeating sampler.
            materials(6),
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
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
            storage(11, compute, true),
            storage(12, compute, false),
            storage(13, compute, false),
            storage(14, compute, true),
            storage(15, compute, false),
            storage(16, compute, true),
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
    let compute_pipeline = |label, entry| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&skin_pipeline_layout),
            module: shader,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let pose_pipeline = compute_pipeline("soldier posing", "cs_pose");
    let skin_pipeline = compute_pipeline("soldier skinning", "cs_skin");
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
            multisample: target.multisample(),
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
        crate::Target::Color(hdr, "fs_soldier_textured"),
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
        crate::Target::Color(hdr, "fs_soldier"),
        None,
        &buffers,
    );
    // Under each soldier a quad on the ground, darkening what is below it.
    // It needs only the instance's place, lift and size.
    let contact_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("soldier contact shadows"),
        layout: Some(world_pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_contact"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<SoldierInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &CONTACT_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            cull_mode: None,
            ..Default::default()
        },
        // Tested against the soldiers and the ground, writing nothing; a
        // little toward the camera so the ground's coarser triangles at a
        // distance do not cut it.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: -4,
                slope_scale: -2.0,
                clamp: 0.0,
            },
        }),
        multisample: hdr.multisample(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_contact"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: hdr.format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
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
        pose_pipeline,
        contact_pipeline,
        skin_pipeline,
        models_layout,
        skin_layout,
        instances,
        models: None,
        draws: Vec::new(),
        jobs: Vec::new(),
        origins: Vec::new(),
        threads: 0,
        posed: 0,
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
        self.posed = 0;
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
            /// The level of detail the caller asked for, before the budget
            /// bumped the run to a coarser one.
            asked: usize,
            levels: [Level; LODS],
            textured: bool,
            bones: usize,
            fit: u32,
        }
        let mut runs = Vec::with_capacity(batches.len());
        for b in batches {
            let Some(&ModelInfo {
                levels,
                textured,
                bones,
                fit,
            }) = models.levels.get(b.model as usize)
            else {
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
                asked: (b.lod as usize).min(LODS - 1),
                levels,
                textured,
                bones: bones as usize,
                fit,
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
        // Then cut soldiers from the back until both the vertices and the
        // bone matrices fit.
        let mut bones: usize = runs.iter().map(|r| r.bones * r.n).sum();
        let mut left_out = 0;
        for r in runs.iter_mut().rev() {
            if total <= SKIN_VERTICES && bones <= PALETTE_BONES {
                break;
            }
            let len = r.levels[r.lod].len as usize;
            let cut = (total.saturating_sub(SKIN_VERTICES).div_ceil(len))
                .max(bones.saturating_sub(PALETTE_BONES).div_ceil(r.bones))
                .min(r.n);
            r.n -= cut;
            total -= cut * len;
            bones -= cut * r.bones;
            left_out += cut;
        }
        // A job per run, as many as the pack's models and levels can make
        // (`set_models` checks that the shader's table holds them).
        debug_assert!(runs.len() <= MAX_JOBS, "more soldier batches than jobs");
        let (mut slots, mut palette, mut posed, mut top) = (0, 0, 0, 0);
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
            self.jobs.push([
                r.first,
                level.first,
                level.len,
                slots as u32,
                palette as u32,
                r.bones as u32,
                r.fit,
                posed as u32,
            ]);
            self.draws.push(Draw {
                first: r.first,
                end: r.first + r.n as u32,
                asked: r.asked,
                mesh: level.mesh,
                shadow_mesh: level.shadow_mesh,
                textured: r.textured,
            });
            slots += r.n * level.len as usize;
            palette += r.n * r.bones;
            posed += r.n;
            top = top.max(r.first as usize + r.n);
        }
        self.threads = slots as u32;
        self.posed = posed as u32;
        if slots > 0 {
            // Room for this frame's vertices (the bind groups change with
            // the buffers, so the borrow is taken afresh).
            if let Some(mut models) = self.models.take() {
                models.grow(device, self, slots, palette);
                self.models = Some(models);
            }
            let models = self.models.as_ref().expect("models were just put back");
            let header = [
                self.jobs.len() as u32,
                self.threads,
                models.split,
                self.posed,
            ];
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

    /// Build the planned soldiers' bone matrices, then pose their vertices
    /// with them. `world` is the camera's bind group, which holds the
    /// terrain the soldiers stand on.
    pub fn skin(&self, encoder: &mut wgpu::CommandEncoder, world: &wgpu::BindGroup) {
        let Some(models) = self.models.as_ref().filter(|_| self.threads > 0) else {
            return;
        };
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("skin soldiers"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, world, &[]);
        pass.set_bind_group(1, &models.skin_group, &[]);
        // Each dispatch is its own usage scope, so the skinning dispatch
        // sees the palette the posing one wrote.
        pass.set_pipeline(&self.pose_pipeline);
        pass.dispatch_workgroups(self.posed.min(POSE_ROW), self.posed.div_ceil(POSE_ROW), 1);
        pass.set_pipeline(&self.skin_pipeline);
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

    /// Soft dark patches on the ground under the soldiers drawn at the finest
    /// levels of detail (farther off they are a pixel), after the soldiers so
    /// they hide them; the number of draw calls.
    pub fn draw_contacts(&self, pass: &mut wgpu::RenderPass<'_>) -> u32 {
        pass.set_pipeline(&self.contact_pipeline);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        let mut calls = 0;
        // Adjacent runs (the batches are usually laid out in instance
        // order) go in one call.
        let mut run = 0..0;
        for d in self.draws.iter().filter(|d| d.asked <= CONTACT_MAX_LOD) {
            if d.first != run.end {
                if !run.is_empty() {
                    pass.draw(0..4, run.clone());
                    calls += 1;
                }
                run.start = d.first;
            }
            run.end = d.end;
        }
        if !run.is_empty() {
            pass.draw(0..4, run);
            calls += 1;
        }
        calls
    }
}

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    soldiers: &SoldiersGpu,
    m: &Models<'_>,
    detail: Option<&gc_assets::detail::Detail<'_>>,
) -> Result<ModelGpu, String> {
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
    // The skinning pass binds the vertices in two halves at most (see
    // `vertex_split`), and the buffer itself must fit.
    let count = levels.vertices.len() as u64;
    let split = vertex_split(device, count);
    let limits = device.limits();
    if count - split > split || count * VERTEX_BYTES > limits.max_buffer_size {
        return Err(format!(
            "{} MB of model vertices, more than this GPU binds ({} MB twice) or holds ({} MB)",
            (count * VERTEX_BYTES) >> 20,
            limits.max_storage_buffer_binding_size >> 20,
            limits.max_buffer_size >> 20
        ));
    }
    let split = split as u32;
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
        16 + MAX_JOBS * JOB_BYTES,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
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
    // Without BC7, or room for every material's layers, the textured
    // models draw in their vertex colours.
    let textures = device
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        && m.materials() > 0
        && m.materials() * LAYERS_PER_MATERIAL <= device.limits().max_texture_array_layers as usize;
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
    let detail = DetailGpu::new(device, queue, detail);
    let (anim_head, fits) = anim_gpu(m);
    let levels_of = levels
        .models
        .iter()
        .zip(m.models)
        .zip(fits)
        .map(|((levels, r), fit)| ModelInfo {
            levels: *levels,
            textured: textures
                && m.vertices
                    .get(r.first_vertex as usize)
                    .is_some_and(|v| v.material > 0),
            bones: r.bones,
            fit,
        });
    // The poses straight from the pack, then the skeletons and fits after.
    let poses = 1 + m.anim.frames.len() as u64;
    let anim_bytes = (poses + anim_head.len() as u64) * 16;
    if anim_bytes > limits.max_storage_buffer_binding_size {
        return Err(format!(
            "{} MB of skeletons and moves, more than this GPU binds ({} MB)",
            anim_bytes >> 20,
            limits.max_storage_buffer_binding_size >> 20
        ));
    }
    let anim = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("skeletons and poses"),
        size: anim_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&anim, 16, bytemuck::cast_slice(m.anim.frames));
    queue.write_buffer(&anim, poses * 16, bytemuck::cast_slice(&anim_head));
    let (skin_pos, skin_shade) = skin_buffers(device, SKIN_START);
    let palette = palette_buffer(device, PALETTE_START);
    let (bind_group, skin_group) = bind_groups(
        device,
        soldiers,
        &Parts {
            vertices: &vertices,
            split,
            origins: &origins,
            jobs: &jobs,
            skin_pos: &skin_pos,
            skin_shade: &skin_shade,
            palette: &palette,
            anim: &anim,
            bones: &view,
            srgb: &srgb_view,
            linear: &linear_view,
            sampler: &sampler,
            detail: &detail.view,
            detail_sampler: &detail.sampler,
        },
    );
    Ok(ModelGpu {
        fixed_bytes: vertices.size()
            + indices.size()
            + origins.size()
            + jobs.size()
            + anim.size()
            + (texels.len() * 8) as u64
            + if textures { size_of_val(m.textures) } else { 4 } as u64,
        vertices,
        indices,
        origins,
        jobs,
        skin_pos,
        skin_shade,
        skin_capacity: SKIN_START,
        palette,
        palette_capacity: PALETTE_START,
        anim,
        bones: view,
        srgb: srgb_view,
        linear: linear_view,
        sampler,
        detail,
        bind_group,
        skin_group,
        levels: levels_of.collect(),
        split,
    })
}

/// The anim buffer the pose pass reads (`anim` in world.wgsl) is vectors
/// of four words: one unused, then every joint pose of the pack (as stored:
/// eight half floats each), then what this returns, which starts right after
/// them, with per model where its fit starts (0 for none):
///
/// - Per skeleton, a vector per joint: its parent (255 for a root), depth,
///   the masks it is in (one bit each), and its role (low byte), mirror
///   (next byte) and fix (the third byte, see [`rig_of`]).
/// - Per fitted model: its skeleton's first joint, joint count, deepest
///   depth and first pose, then its skeleton's rig ([`rig_of`]), then its
///   skeleton-to-model matrix and per bone its fix, three rows of floats
///   each.
fn anim_gpu(m: &Models<'_>) -> (Vec<[u32; 4]>, Vec<u32>) {
    let start = 1 + m.anim.frames.len() as u32;
    let mut out = Vec::new();
    let mut joints_at = Vec::with_capacity(m.anim.skeletons.len());
    let mut deepest = Vec::with_capacity(m.anim.skeletons.len());
    let mut rigs = Vec::with_capacity(m.anim.skeletons.len());
    for s in 0..m.anim.skeletons.len() {
        joints_at.push(start + out.len() as u32);
        let joints = m.anim.joints_of(s);
        // The loader has checked that the parents do not loop.
        let depths = gc_assets::anim::depths(joints).unwrap_or_default();
        deepest.push(depths.iter().copied().max().map_or(0, u32::from));
        let (head, fixes) = rig_of(joints);
        rigs.push(head);
        for ((j, depth), fix) in joints.iter().zip(depths).zip(fixes) {
            out.push([
                u32::from(j.parent),
                u32::from(depth),
                u32::from(j.masks),
                u32::from(j.role) | u32::from(j.mirror) << 8 | u32::from(fix) << 16,
            ]);
        }
    }
    let fits = (0..m.models.len())
        .map(|model| {
            let Some(fit) = m.anim.fit(model) else {
                return 0;
            };
            let at = start + out.len() as u32;
            let s = fit.skeleton as usize;
            let skeleton = &m.anim.skeletons[s];
            out.push([
                joints_at[s],
                skeleton.joints,
                deepest[s],
                1 + skeleton.first_pose,
            ]);
            out.push(rigs[s]);
            let fixes =
                &m.anim.fixes[fit.first_fix as usize..][..1 + m.models[model].bones as usize];
            out.extend_from_slice(bytemuck::cast_slice::<_, [u32; 4]>(fixes));
            at
        })
        .collect();
    (out, fits)
}

/// A joint missing from a skeleton's rig.
const NO_JOINT: u32 = 255;

/// The joints `cs_pose` bends for foot IK and turns for look-at, and which
/// of its fixes each joint takes. The rig is one vector: the left leg's
/// upper, lower and foot joints (a byte each; `NO_JOINT` for both legs
/// unless each is a chain it can bend), the right leg's, then chest, neck
/// and head. A joint takes the fix
/// of the nearest of those at or above it: 1 to 3 the left leg's upper, lower
/// and foot, 4 to 6 the right's, 7 to 9 chest, neck and head, 0 none (only
/// the drop of the whole body).
fn rig_of(joints: &[JointRecord]) -> ([u32; 4], Vec<u8>) {
    let find = |r: Role| joints.iter().position(|j| j.role == r as u8);
    let leg = |upper, lower, foot| {
        let (u, l, f) = (find(upper)?, find(lower)?, find(foot)?);
        // Only a chain the two-bone solve can bend.
        (usize::from(joints[f].parent) == l && usize::from(joints[l].parent) == u)
            .then_some([u, l, f])
    };
    let mut fixed = [None; 9];
    // Both legs or neither: the body drops to the lower foot, so a leg that
    // cannot bend would sink into the ground.
    if let (Some(l), Some(r)) = (
        leg(Role::UpperLegL, Role::LowerLegL, Role::FootL),
        leg(Role::UpperLegR, Role::LowerLegR, Role::FootR),
    ) {
        fixed[..3].copy_from_slice(&l.map(Some));
        fixed[3..6].copy_from_slice(&r.map(Some));
    }
    for (k, r) in [Role::Chest, Role::Neck, Role::Head]
        .into_iter()
        .enumerate()
    {
        fixed[6 + k] = find(r);
    }
    let byte = |j: Option<usize>| j.map_or(NO_JOINT, |j| j as u32);
    let word = |k: usize| byte(fixed[k]) | byte(fixed[k + 1]) << 8 | byte(fixed[k + 2]) << 16;
    let fixes = (0..joints.len())
        .map(|j| {
            let mut at = j;
            // The loader has checked that the parents do not loop.
            for _ in 0..joints.len() {
                if let Some(k) = fixed.iter().position(|&f| f == Some(at)) {
                    return k as u8 + 1;
                }
                match joints[at].parent {
                    p if usize::from(p) < joints.len() => at = usize::from(p),
                    _ => break,
                }
            }
            0
        })
        .collect();
    ([word(0), word(3), word(6), 0], fixes)
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
    upload_bc7_layers(queue, &texture, m.textures, TEXTURE_SIZE, TEXTURE_MIPS);
    texture
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pitch_is_a_tip_toward_the_back_for_a_climb_and_the_front_for_a_descent() {
        assert_eq!(pitch_bytes(0.0)[0], 0, "level is upright");
        let up = pitch_bytes(0.2);
        let down = pitch_bytes(-0.2);
        assert_eq!(up[0], down[0]);
        assert!((32..=33).contains(&up[0]), "{up:?}");
        assert_eq!((up[1], down[1]), (128, 0));
        // Never the quarter turn that means lying down.
        assert_eq!(pitch_bytes(5.0)[0], 254);
    }

    #[test]
    fn the_rig_names_the_legs_it_can_bend_and_each_joint_takes_its_fix() {
        let j = |parent: u8, role: Role| JointRecord {
            parent,
            masks: 1,
            mirror: 0,
            role: role as u8,
        };
        // hips, chest, head, left leg (upper, lower, foot, toe), a right leg
        // whose foot hangs off the upper leg, and a hand on the chest.
        let mut joints = vec![
            j(gc_assets::anim::ROOT, Role::Hips),
            j(0, Role::Chest),
            j(1, Role::Head),
            j(0, Role::UpperLegL),
            j(3, Role::LowerLegL),
            j(4, Role::FootL),
            j(5, Role::None),
            j(0, Role::UpperLegR),
            j(7, Role::LowerLegR),
            j(7, Role::FootR),
            j(1, Role::None),
        ];
        let none = NO_JOINT | NO_JOINT << 8 | NO_JOINT << 16;
        // One leg that cannot bend leaves both alone.
        let (head, fixes) = rig_of(&joints);
        assert_eq!(head[..2], [none, none]);
        assert_eq!(head[2], 1 | NO_JOINT << 8 | 2 << 16);
        assert_eq!(fixes, vec![0, 7, 9, 0, 0, 0, 0, 0, 0, 0, 7]);
        // With the right foot on its lower leg, both legs bend.
        joints[9].parent = 8;
        let (head, fixes) = rig_of(&joints);
        assert_eq!(head[0], 3 | 4 << 8 | 5 << 16);
        assert_eq!(head[1], 7 | 8 << 8 | 9 << 16);
        assert_eq!(fixes, vec![0, 7, 9, 1, 2, 3, 3, 4, 5, 6, 7]);
    }
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
            anim: Default::default(),
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
