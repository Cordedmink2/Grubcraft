//! Grubcraft renderer: wgpu on Metal (macOS), DirectX 12 (Windows) and Vulkan.
//!
//! Current scope: heightmap terrain drawn in frustum-culled chunks, soldier
//! models animated from a bone texture (one instanced draw per model and
//! level of detail), and instanced boxes for markers, arrows and unit types
//! without a model, all lit by the sun with a shadow map, drawn in HDR and
//! tone-mapped with a glow around the brightest light. See
//! `docs/10-architecture.md#rendering` for where this is heading.
//!
//! The renderer knows nothing about windows or input: the caller hands it a
//! surface target and per-frame camera and instance data.

mod debug_font;
mod overlay;
mod post;
mod scenery;
mod shadows;
mod soldiers;

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

/// Tiles per terrain chunk side (chunks are the unit of culling).
pub const CHUNK_TILES: u32 = 32;
/// Quads per chunk side at the finest level of detail; vertices sit every
/// half tile.
const CHUNK_QUADS: u32 = 64;
/// Levels of detail a chunk can be drawn at. Level `n` has `CHUNK_QUADS >> n`
/// quads a side: 64, 32, 16 and 8, so a vertex every half tile, tile, two
/// tiles and four tiles. Each level's vertices are every second vertex of
/// the one below, which is what lets neighbours of different levels meet
/// without cracks (see `vs_terrain` in world.wgsl).
const TERRAIN_LEVELS: usize = 4;
// A level takes two bits of the packed instance word (`chunk_instance`).
const _: () = assert!(TERRAIN_LEVELS <= 4);
/// How far from the eye (in tiles, to the nearest point of the chunk's box)
/// a chunk stays at level 0, 1 and 2; beyond the last it is level 3.
/// Level 0 reaches four chunks out. At the play camera's default zoom (110
/// tiles from the ground it looks at) the ground from the bottom of the
/// screen to a little past the middle stays at today's detail and only the
/// far strip along the top steps down; zoomed in closer than about 90
/// tiles, everything on screen is level 0. The steps are further apart
/// than a chunk's diagonal (45 tiles), so neighbouring chunks differ by at
/// most one level and the snapped edge strip stays narrow. Past a hundred
/// tiles a level 1 quad (one tile) is only a few pixels across, and the
/// per-pixel normals in `fs_terrain` keep the lighting the same at every
/// level.
const LOD_DISTANCES: [f32; TERRAIN_LEVELS - 1] = [128.0, 256.0, 384.0];
/// Bytes of per-chunk instance data: the chunk's coordinates and a word
/// packing its level and its four neighbours' levels.
const CHUNK_INSTANCE_BYTES: u64 = 12;
/// Fixed capacity of the box instance buffer (soldiers and markers); never reallocated.
pub const MAX_BOX_INSTANCES: usize = 65_536;
pub use overlay::{MAX_OVERLAY_CHARS, UiQuad};
/// Most quads one frame of menu UI may draw.
pub const MAX_UI_QUADS: usize = 8192;
pub use scenery::{MAX_SCENERY_INSTANCES, SceneryBatch, SceneryInstance};
pub use soldiers::{MAX_SOLDIER_INSTANCES, SoldierBatch, SoldierInstance};
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
use shadows::{SHADOW_BIAS, SHADOW_FORMAT, SHADOW_SIZE};
/// Direction toward the sun.
const SUN: Vec3 = Vec3::new(-0.45, 0.75, -0.35);
/// The sky (and fog) colour, in sRGB.
const SKY: [f32; 3] = [0.62, 0.66, 0.70];

#[derive(Debug)]
pub enum InitError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    /// The GPU lacks something soldier skinning needs.
    Unsupported(&'static str),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Surface(e) => write!(f, "could not create a drawing surface: {e}"),
            InitError::Adapter(e) => write!(f, "no suitable GPU found: {e}"),
            InitError::Device(e) => write!(f, "could not open the GPU: {e}"),
            InitError::Unsupported(what) => write!(f, "this GPU cannot {what}"),
        }
    }
}

impl std::error::Error for InitError {}

/// Terrain heightmap: `size × size` samples, `samples_per_tile` per tile edge.
pub struct Terrain<'a> {
    pub size: u32,
    pub samples_per_tile: u32,
    /// Height of a sample value of `u16::MAX`, in tiles.
    pub max_height: f32,
    pub water_level: f32,
    pub heights: &'a [u16],
    /// The colour of the grass by the land it grows in, sRGB, `tint_side`
    /// texels a side over the whole map, row by row; empty for the plain
    /// palette. The terrain shader blends between texels, so borders
    /// between lands are soft.
    pub tint: &'a [[u8; 4]],
    pub tint_side: u32,
}

/// One shaded box draped on the terrain: a soldier, a ground marker or an
/// arrow in flight. 36 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct BoxInstance {
    /// Centre on the map, in tiles.
    pub pos: [f32; 2],
    /// Unit vector the box faces.
    pub facing: [f32; 2],
    /// Width (across the facing), height and depth in tiles.
    pub size: [f32; 3],
    /// Team colour (rgb) and selection highlight (a).
    pub color: [u8; 4],
    /// Height above the ground, in tiles (arrows in flight).
    pub lift: f32,
}

/// How the renderer is set up. The draw switches exist to measure what each
/// part of the frame costs on real machines.
#[derive(Clone, Copy, Debug)]
pub struct RendererOptions {
    /// Frames the GPU may queue (1 to 3). Each holds a full-window image.
    pub frame_latency: u32,
    /// Fraction of the window's pixels to render (0.25 to 1.0); the system
    /// compositor scales the image up to fill the window.
    pub render_scale: f32,
    pub draw_terrain: bool,
    pub draw_blocks: bool,
    /// The sun casts shadows (an 8 MB shadow map drawn every frame).
    pub shadows: bool,
    /// Bright light glows (a chain of half-size images every frame).
    pub glow: bool,
}

impl Default for RendererOptions {
    fn default() -> Self {
        RendererOptions {
            frame_latency: 2,
            render_scale: 1.0,
            draw_terrain: true,
            draw_blocks: true,
            shadows: true,
            glow: true,
        }
    }
}

pub struct FrameParams {
    pub view_proj: Mat4,
    pub camera_pos: Vec3,
    pub time_seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOutcome {
    Presented,
    /// Nothing drawn this frame (window hidden, surface busy or reconfigured).
    Skipped,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub draw_calls: u32,
    pub chunks_drawn: u32,
    pub chunks_total: u32,
    pub boxes_drawn: u32,
    pub soldiers_drawn: u32,
    /// Scenery instances drawn in the world pass (trees, bushes, grass, rocks).
    pub scenery_drawn: u32,
    /// Triangles drawn in the world pass, soldiers and scenery.
    pub triangles: u64,
    /// Boxes and soldiers asked for beyond the fixed instance buffers this
    /// frame, so left undrawn. Zero unless a match outgrows the capacity.
    pub dropped: u32,
}

/// What a frame draws, for leaving out the soldiers nobody would see: the
/// camera's view and, with shadows on, the sun's shadow map (a soldier
/// outside the screen can still cast onto it).
#[derive(Clone, Copy, Debug)]
pub struct Cull {
    camera: [Vec4; 6],
    sun: Option<[Vec4; 6]>,
}

impl Cull {
    /// Sees everything; for before the first frame.
    pub const EVERYTHING: Cull = Cull {
        camera: [Vec4::new(0.0, 0.0, 0.0, 1.0); 6],
        sun: None,
    };

    fn new(camera: Mat4, sun: Option<Mat4>) -> Cull {
        // A broken matrix (a window with no size yet) draws everything
        // rather than nothing.
        if !camera.is_finite() || sun.is_some_and(|m| !m.is_finite()) {
            return Cull::EVERYTHING;
        }
        Cull {
            camera: unit_planes(frustum_planes(camera)),
            sun: sun.map(|m| unit_planes(frustum_planes(m))),
        }
    }

    /// Whether a sphere of `radius` around `center` touches the camera's
    /// view (not only the sun's).
    pub fn on_screen(&self, center: Vec3, radius: f32) -> bool {
        self.camera
            .iter()
            .all(|p| p.truncate().dot(center) + p.w >= -radius)
    }

    /// Whether a sphere of `radius` around `center` touches what is drawn.
    pub fn sees(&self, center: Vec3, radius: f32) -> bool {
        let inside = |planes: &[Vec4; 6]| {
            planes
                .iter()
                .all(|p| p.truncate().dot(center) + p.w >= -radius)
        };
        inside(&self.camera) || self.sun.as_ref().is_some_and(inside)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    sun_dir: [f32; 4],
    map: [f32; 4],
    fog: [f32; 4],
    misc: [f32; 4],
    sun_view_proj: [[f32; 4]; 4],
    shadow: [f32; 4],
}

struct TerrainGpu {
    bind_group: wgpu::BindGroup,
    /// The same with the sun's view, for drawing the shadow map.
    shadow_bind_group: wgpu::BindGroup,
    tiles: u32,
    chunks_per_side: u32,
    max_height: f32,
    samples_per_tile: u32,
    water_level: f32,
    /// One texel per tile: how much of the ground is in view (see
    /// [`Renderer::set_fog`]).
    fog_map: wgpu::Texture,
    /// Whether the fog map holds anything but all-clear.
    fogged: bool,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    options: RendererOptions,
    /// How far from the camera the shadow map reaches, in tiles.
    shadow_range: f32,
    depth_view: wgpu::TextureView,
    depth_bytes: u64,
    globals: wgpu::Buffer,
    /// `Globals` as the sun sees the map, for the shadow pass.
    shadow_globals: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    terrain_pipeline: wgpu::RenderPipeline,
    block_pipeline: wgpu::RenderPipeline,
    terrain_shadow_pipeline: wgpu::RenderPipeline,
    block_shadow_pipeline: wgpu::RenderPipeline,
    shadow_map: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    /// Soft edges for the fog map, and the ground tint blended between
    /// its texels.
    soft_sampler: wgpu::Sampler,
    /// Bound where the shadow map would be while the shadow map is drawn.
    no_shadow_map: wgpu::TextureView,
    post: post::PostGpu,
    post_targets: post::PostTargets,
    /// The sky colour, scaled so that it tone-maps back to itself.
    fog_color: [f32; 3],
    /// The chunk grid's triangles at every level of detail, one after the
    /// other (see [`level_indices`]).
    chunk_indices: wgpu::Buffer,
    chunk_instances: wgpu::Buffer,
    chunk_instance_capacity: u32,
    box_instances: wgpu::Buffer,
    terrain: Option<TerrainGpu>,
    /// GPU memory we allocated ourselves (excludes the swapchain and driver).
    static_gpu_bytes: u64,
    terrain_gpu_bytes: u64,
    stats: RenderStats,
    adapter_info: wgpu::AdapterInfo,
    /// What the GPU driver reported behind our back; see [`GpuHealth`].
    health: Arc<GpuHealth>,
    /// Scratch lists of the chunks the camera and the sun see, by level,
    /// reused every frame.
    visible_chunks: ChunkList,
    sun_chunks: ChunkList,
    /// Each chunk's level of detail this frame, row by row; the same for
    /// the camera's pass and the sun's, so the shadows are cast by the
    /// surface that is drawn.
    chunk_levels: Vec<u8>,
    overlay: OverlayGpu,
    soldiers: soldiers::SoldiersGpu,
    scenery: scenery::SceneryGpu,
    /// Menu and lobby screens, drawn under the dev overlay.
    ui: OverlayGpu,
}

/// How many driver messages are kept between two looks at them, and how
/// many different ones are remembered so a repeating error is logged once.
const MAX_GPU_MESSAGES: usize = 16;
const MAX_GPU_DISTINCT: usize = 64;

/// Problems the GPU driver reports from its own threads: a lost device, or
/// (in release builds, where we log instead of panicking) an error that no
/// code was waiting for. The game polls it once a frame.
#[derive(Default)]
struct GpuHealth {
    lost: AtomicBool,
    messages: Mutex<GpuMessages>,
}

#[derive(Default)]
struct GpuMessages {
    /// Not yet taken by the game.
    pending: Vec<String>,
    /// Everything reported so far; a repeat is dropped, since an error that
    /// recurs every frame would otherwise flood the log.
    seen: Vec<String>,
}

impl GpuHealth {
    fn report(&self, message: String) {
        let Ok(mut m) = self.messages.lock() else {
            return;
        };
        if m.seen.contains(&message) || m.seen.len() >= MAX_GPU_DISTINCT {
            return;
        }
        m.seen.push(message.clone());
        if m.pending.len() < MAX_GPU_MESSAGES {
            m.pending
                .push(format!("{message} (repeats are not logged again)"));
        }
    }
}

struct OverlayGpu {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    glyphs: Vec<UiQuad>,
    count: u32,
    scale: f32,
    gpu_bytes: u64,
}

impl Renderer {
    /// Create the renderer for a window (or any surface target).
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        window_width: u32,
        window_height: u32,
        options: RendererOptions,
    ) -> Result<Renderer, InitError> {
        let options = RendererOptions {
            frame_latency: options.frame_latency.clamp(1, 3),
            render_scale: options.render_scale.clamp(0.25, 1.0),
            ..options
        };
        let (width, height) = scaled(window_width, window_height, options.render_scale);
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        desc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(desc);
        let surface = instance
            .create_surface(target)
            .map_err(InitError::Surface)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                // Integrated GPUs are the target and save laptop battery.
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(InitError::Adapter)?;
        // Soldiers are posed by a compute pass into buffers that the vertex
        // shaders read. Vulkan, Metal and Direct3D 12 all can.
        let downlevel = adapter.get_downlevel_capabilities().flags;
        for (flag, what) in [
            (wgpu::DownlevelFlags::COMPUTE_SHADERS, "run compute shaders"),
            (
                wgpu::DownlevelFlags::VERTEX_STORAGE,
                "read storage buffers in vertex shaders",
            ),
        ] {
            if !downlevel.contains(flag) {
                return Err(InitError::Unsupported(what));
            }
        }
        // The HDR image at 4 bytes a pixel where the GPU can draw into that
        // format (Apple, most desktop GPUs), else 8.
        let small_hdr = adapter
            .features()
            .contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE);
        // Soldier textures are BC7, which every Mac and Windows GPU we
        // target reads; without it textured models fall back to their
        // vertex colours.
        let wanted =
            wgpu::Features::RG11B10UFLOAT_RENDERABLE | wgpu::Features::TEXTURE_COMPRESSION_BC;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("grubcraft"),
                required_features: adapter.features() & wanted,
                required_limits: wgpu::Limits {
                    // Soldier and building materials take three layers
                    // each: ask for all the GPU has (2,048 on every desktop
                    // GPU we target).
                    max_texture_array_layers: adapter.limits().max_texture_array_layers,
                    // The skinning pass reads every model vertex, buildings
                    // included: as much at once as the GPU binds (it splits
                    // them in two where that is under the vertices' size).
                    max_storage_buffer_binding_size: adapter
                        .limits()
                        .max_storage_buffer_binding_size,
                    max_buffer_size: adapter.limits().max_buffer_size,
                    // Five for the skinning pass (the model vertices take
                    // two); every desktop GPU has at least eight.
                    max_storage_buffers_per_shader_stage: adapter
                        .limits()
                        .max_storage_buffers_per_shader_stage,
                    ..wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits())
                },
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .map_err(InitError::Device)?;
        let health = Arc::new(GpuHealth::default());
        let lost = health.clone();
        device.set_device_lost_callback(move |reason, message| {
            // Destroyed is our own teardown at exit, not a failure.
            if matches!(reason, wgpu::DeviceLostReason::Destroyed) {
                return;
            }
            // Never deduplicated or dropped: it is why the game is quitting.
            if let Ok(mut m) = lost.messages.lock() {
                m.pending
                    .push(format!("GPU device lost ({reason:?}): {message}"));
            }
            lost.lost.store(true, Ordering::Release);
        });
        // wgpu's default handler panics. Debug builds keep that, so a
        // validation error is a loud bug. A release build logs it and carries
        // on, since one bad draw is better than a crash in a friend's game.
        if !cfg!(debug_assertions) {
            let errors = health.clone();
            device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
                errors.report(format!("GPU error: {e}"));
            }));
        }

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            // Frames in flight. Each one can hold a full-window drawable
            // (about 15 MB at Retina resolution).
            desired_maximum_frame_latency: options.frame_latency,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            ..surface.get_default_config(&adapter, width, height).unwrap()
        };
        surface.configure(&device, &config);

        let uniform = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals = uniform("globals");
        let shadow_globals = uniform("sun globals");

        // The skinning pass stands soldiers on the terrain too.
        let world_stages = wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: world_stages,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<Globals>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: world_stages,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });

        let hdr_format = if small_hdr {
            wgpu::TextureFormat::Rg11b10Ufloat
        } else {
            wgpu::TextureFormat::Rgba16Float
        };
        let terrain = |color| {
            make_pipeline(
                &device,
                &layout,
                &shader,
                "terrain",
                "vs_terrain",
                color,
                wgpu::VertexBufferLayout {
                    array_stride: CHUNK_INSTANCE_BYTES,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Uint32x2, 1 => Uint32],
                },
            )
        };
        let terrain_pipeline = terrain(Target::Color(hdr_format, "fs_terrain"));
        let terrain_shadow_pipeline = terrain(Target::Shadow);
        let block = |color| {
            make_pipeline(
                &device,
                &layout,
                &shader,
                "boxes",
                "vs_block",
                color,
                wgpu::VertexBufferLayout {
                    array_stride: size_of::<BoxInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2, 1 => Float32x2, 2 => Float32x3, 3 => Unorm8x4,
                        4 => Float32
                    ],
                },
            )
        };
        let block_pipeline = block(Target::Color(hdr_format, "fs_block"));
        let block_shadow_pipeline = block(Target::Shadow);

        let soldiers = soldiers::make(&device, &bind_group_layout, &shader, hdr_format);
        let scenery = scenery::make(&device, &layout, hdr_format);
        let shadow_texture = |label, side| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: side,
                        height: side,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SHADOW_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let shadow_side = if options.shadows { SHADOW_SIZE } else { 1 };
        let shadow_map = shadow_texture("sun shadow map", shadow_side);
        let no_shadow_map = shadow_texture("no shadow map", 1);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let soft_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fog of war and ground tint"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post = post::make(&device, hdr_format, format, options.glow);
        let post_targets = post.targets(&device, config.width, config.height);

        let chunk_indices = {
            let indices: Vec<u16> = (0..TERRAIN_LEVELS)
                .flat_map(|level| chunk_grid_indices(CHUNK_QUADS >> level))
                .collect();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunk grid indices"),
                size: (indices.len() * 2) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&indices));
            buffer
        };

        let box_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("box instances"),
            size: (MAX_BOX_INSTANCES * size_of::<BoxInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Placeholder until terrain arrives; replaced by `set_terrain`.
        let chunk_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk instances"),
            size: CHUNK_INSTANCE_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let overlay = make_overlay(&device, &queue, format, MAX_OVERLAY_CHARS);
        let ui = make_overlay(&device, &queue, format, MAX_UI_QUADS);
        let adapter_info = adapter.get_info();
        let (depth_view, depth_bytes) = make_depth(
            &device,
            &config,
            adapter_info.transient_saves_memory == Some(true),
        );
        let static_gpu_bytes = 2 * size_of::<Globals>() as u64
            + u64::from(shadow_side)
                * u64::from(shadow_side)
                * u64::from(SHADOW_FORMAT.block_copy_size(None).unwrap_or(4))
            + chunk_indices.size()
            + box_instances.size()
            + soldiers.instances.size()
            + scenery.instance_bytes()
            + chunk_instances.size();

        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            options,
            shadow_range: shadows::SHADOW_RANGE,
            depth_view,
            depth_bytes,
            globals,
            shadow_globals,
            bind_group_layout,
            terrain_pipeline,
            block_pipeline,
            terrain_shadow_pipeline,
            block_shadow_pipeline,
            shadow_map,
            shadow_sampler,
            soft_sampler,
            no_shadow_map,
            post,
            post_targets,
            fog_color: SKY.map(|c| post::untonemap(srgb_to_linear(c))),
            chunk_indices,
            chunk_instances,
            chunk_instance_capacity: 1,
            box_instances,
            terrain: None,
            static_gpu_bytes,
            terrain_gpu_bytes: 0,
            stats: RenderStats::default(),
            adapter_info,
            health,
            visible_chunks: ChunkList::default(),
            sun_chunks: ChunkList::default(),
            chunk_levels: Vec::new(),
            overlay,
            soldiers,
            scenery,
            ui,
        })
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// True once the GPU device is gone (driver reset, GPU unplugged). The
    /// renderer cannot draw any more; the game should save and leave.
    pub fn device_lost(&self) -> bool {
        self.health.lost.load(Ordering::Acquire)
    }

    /// Take the driver messages (device loss, uncaptured errors) gathered
    /// since the last call, for the game to log.
    pub fn take_gpu_messages(&self) -> Vec<String> {
        self.health
            .messages
            .lock()
            .map(|mut m| std::mem::take(&mut m.pending))
            .unwrap_or_default()
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// What a frame seen through `view_proj` will draw, for [`Cull::sees`].
    pub fn cull(&self, view_proj: Mat4) -> Cull {
        let sun = self
            .terrain
            .as_ref()
            .and_then(|t| self.sun_view(view_proj, t))
            .map(|(m, _)| m);
        Cull::new(view_proj, sun)
    }

    /// The sun's view and projection, and the size of one shadow texel in
    /// tiles, for a camera seen through `view_proj`; none with shadows off.
    fn sun_view(&self, view_proj: Mat4, terrain: &TerrainGpu) -> Option<(Mat4, f32)> {
        self.options.shadows.then(|| {
            shadows::sun_view_proj(
                view_proj,
                SUN.normalize(),
                terrain.tiles as f32,
                terrain.max_height,
                self.shadow_range,
            )
        })
    }

    /// Reach the sun's shadow map out to `tiles` from the camera, or back to
    /// the default (the bird's-eye camera's widest view) with `None`. A
    /// low camera that sees a long strip of ground asks for less, so the
    /// map's texels go to the soldiers near it.
    pub fn set_shadow_range(&mut self, tiles: Option<f32>) {
        self.shadow_range = tiles.unwrap_or(shadows::SHADOW_RANGE);
    }

    /// GPU memory allocated by the renderer itself, in bytes: the sizes of the
    /// buffers and textures it creates, added up by hand (there is no
    /// allocation wrapper), so a figure to read, not to enforce.
    pub fn gpu_bytes(&self) -> u64 {
        self.static_gpu_bytes
            + self.terrain_gpu_bytes
            + self.depth_bytes
            + self.post_targets.gpu_bytes
            + self.overlay.gpu_bytes
            + self.ui.gpu_bytes
            + self.scenery.model_bytes()
            + self
                .soldiers
                .models
                .as_ref()
                .map_or(0, soldiers::ModelGpu::gpu_bytes)
    }

    /// Upload the soldier models and their bone animation texture.
    pub fn set_models(&mut self, models: &gc_assets::model::Models<'_>) -> Result<(), String> {
        let limit = self.device.limits().max_texture_dimension_2d;
        if models.bone_rows() > limit {
            return Err(format!(
                "{} bone animation rows, more than this GPU's {limit}",
                models.bone_rows()
            ));
        }
        self.soldiers.models = Some(soldiers::upload(
            &self.device,
            &self.queue,
            &self.soldiers,
            models,
        )?);
        Ok(())
    }

    /// Upload the scenery models (trees, bushes, grass, rocks) once.
    pub fn set_scenery(&mut self, scenery: &gc_assets::scenery::Scenery<'_>) -> Result<(), String> {
        self.scenery.upload(&self.device, &self.queue, scenery)
    }

    /// Hand over the frame's scenery: `instances` in runs, each run a batch
    /// (a model at a level of detail) the next [`Renderer::render`] draws.
    /// Instances past [`MAX_SCENERY_INSTANCES`] are left out and counted in
    /// [`RenderStats::dropped`]. Does nothing before [`Renderer::set_scenery`].
    pub fn set_scenery_frame(&mut self, instances: &[SceneryInstance], batches: &[SceneryBatch]) {
        self.scenery.set_frame(&self.queue, instances, batches);
    }

    /// Show `text` in the dev overlay (lines separated by `\n`), or hide it
    /// with `None`. `scale` is the pixel size of one font texel, usually the
    /// display's scale factor; the panel starts `top` window pixels down.
    /// Call when the text changes, not every frame.
    pub fn set_overlay(&mut self, text: Option<&str>, scale: f32, top: f32) {
        let o = &mut self.overlay;
        o.scale = (scale * self.options.render_scale).round().max(1.0);
        match text {
            Some(text) => {
                let top = (top * self.options.render_scale / o.scale).ceil() as u16;
                overlay::layout(text, top, &mut o.glyphs);
                o.count = o.glyphs.len() as u32;
                if o.count > 0 {
                    self.queue
                        .write_buffer(&o.instances, 0, bytemuck::cast_slice(&o.glyphs));
                }
            }
            None => o.count = 0,
        }
    }

    /// Show menu UI: `quads` in UI pixels, drawn at `scale` physical pixels
    /// per UI pixel (not rounded, so UI scales like 125% work). Call every
    /// frame the UI is up, and [`Renderer::clear_ui`] when it closes.
    pub fn set_ui(&mut self, quads: &[UiQuad], scale: f32) {
        let u = &mut self.ui;
        u.scale = (scale * self.options.render_scale).max(0.25);
        let quads = &quads[..quads.len().min(MAX_UI_QUADS)];
        u.count = quads.len() as u32;
        if !quads.is_empty() {
            self.queue
                .write_buffer(&u.instances, 0, bytemuck::cast_slice(quads));
        }
    }

    pub fn clear_ui(&mut self) {
        self.ui.count = 0;
    }

    /// Follow a window resize; sizes are the window's, in physical pixels.
    pub fn resize(&mut self, window_width: u32, window_height: u32) {
        if window_width == 0 || window_height == 0 {
            return;
        }
        let (width, height) = scaled(window_width, window_height, self.options.render_scale);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let (view, bytes) = make_depth(
            &self.device,
            &self.config,
            self.adapter_info.transient_saves_memory == Some(true),
        );
        self.depth_view = view;
        self.depth_bytes = bytes;
        self.post_targets = self.post.targets(&self.device, width, height);
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    /// Upload the map's heightmap. The CPU copy can be dropped afterwards
    /// unless the caller needs it (picking, pathfinding).
    pub fn set_terrain(&mut self, t: &Terrain<'_>) {
        assert_eq!(t.heights.len(), (t.size * t.size) as usize);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("heightmap"),
            size: wgpu::Extent3d {
                width: t.size,
                height: t.size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(t.heights),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(t.size * 2),
                rows_per_image: Some(t.size),
            },
            wgpu::Extent3d {
                width: t.size,
                height: t.size,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // The ground tint; a single clear texel when there is none, which the
        // shader reads as "plain palette".
        let (tint_side, tint): (u32, &[[u8; 4]]) = if t.tint.is_empty() {
            (1, &[[0; 4]])
        } else {
            assert_eq!(t.tint.len(), (t.tint_side * t.tint_side) as usize);
            (t.tint_side, t.tint)
        };
        let tint_extent = wgpu::Extent3d {
            width: tint_side,
            height: tint_side,
            depth_or_array_layers: 1,
        };
        let tint_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ground tint"),
            size: tint_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tint_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(tint),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tint_side * 4),
                rows_per_image: Some(tint_side),
            },
            tint_extent,
        );
        let tint_view = tint_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // All clear until `set_fog` says otherwise.
        let tiles = (t.size - 1) / t.samples_per_tile;
        let fog_map = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fog of war"),
            size: wgpu::Extent3d {
                width: tiles,
                height: tiles,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self::write_fog(
            &self.queue,
            &fog_map,
            tiles,
            &vec![255; (tiles * tiles) as usize],
        );
        let fog_view = fog_map.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = |globals: &wgpu::Buffer, shadow_map| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("world"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: globals.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(shadow_map),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&fog_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&self.soft_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&tint_view),
                    },
                ],
            })
        };
        let shadow_bind_group = bind_group(&self.shadow_globals, &self.no_shadow_map);
        let bind_group = bind_group(&self.globals, &self.shadow_map);

        let chunks_per_side = tiles.div_ceil(CHUNK_TILES);
        let chunk_count = chunks_per_side * chunks_per_side;
        self.static_gpu_bytes -= self.chunk_instances.size();
        // The camera's visible chunks, then the sun's.
        self.chunk_instances = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk instances"),
            size: u64::from(chunk_count) * 2 * CHUNK_INSTANCE_BYTES,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.static_gpu_bytes += self.chunk_instances.size();
        self.chunk_instance_capacity = chunk_count;
        self.visible_chunks = ChunkList::with_capacity(chunk_count as usize);
        self.sun_chunks = ChunkList::with_capacity(chunk_count as usize);
        self.chunk_levels = vec![0; chunk_count as usize];
        self.terrain_gpu_bytes = t.heights.len() as u64 * 2
            + u64::from(tint_side * tint_side) * 4
            + u64::from(tiles * tiles);
        self.terrain = Some(TerrainGpu {
            bind_group,
            shadow_bind_group,
            tiles,
            chunks_per_side,
            max_height: t.max_height,
            samples_per_tile: t.samples_per_tile,
            water_level: t.water_level,
            fog_map,
            fogged: false,
        });
    }

    fn write_fog(queue: &wgpu::Queue, texture: &wgpu::Texture, tiles: u32, values: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tiles),
                rows_per_image: Some(tiles),
            },
            wgpu::Extent3d {
                width: tiles,
                height: tiles,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Draw the ground as fog of war has it: one byte per map tile, row by
    /// row from the north-west corner, 255 for ground in view, down to 0 for
    /// ground never seen (about 140 is seen before but not now). The edges
    /// are blurred over about a tile. `None` is no fog at all, as after
    /// `set_terrain`; it costs nothing when the ground is already clear.
    /// Cheap enough to call several times a second. Does nothing before
    /// `set_terrain`, or when `values` is not `tiles × tiles` long.
    pub fn set_fog(&mut self, values: Option<&[u8]>) {
        let Some(terrain) = &mut self.terrain else {
            return;
        };
        let tiles = terrain.tiles;
        let n = (tiles * tiles) as usize;
        match values {
            Some(v) if v.len() == n => Self::write_fog(&self.queue, &terrain.fog_map, tiles, v),
            None if terrain.fogged => {
                Self::write_fog(&self.queue, &terrain.fog_map, tiles, &vec![255; n]);
            }
            _ => return,
        }
        terrain.fogged = values.is_some();
    }

    /// Draw a frame: the terrain, `soldiers` drawn by `batches` (each names
    /// a model and a run of `soldiers`), and `boxes`.
    pub fn render(
        &mut self,
        frame: &FrameParams,
        boxes: &[BoxInstance],
        soldiers: &[SoldierInstance],
        batches: &[SoldierBatch],
    ) -> FrameOutcome {
        let Some(terrain) = &self.terrain else {
            return FrameOutcome::Skipped;
        };
        let mut reconfigure = false;
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                // Draw this one, reconfigure for the next. A scaled-down
                // surface is "suboptimal" on some platforms by design.
                reconfigure = self.options.render_scale >= 1.0;
                t
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return FrameOutcome::Skipped;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return FrameOutcome::Skipped,
        };

        let fog_color = self.fog_color;
        let sun = SUN.normalize();
        let shadows = self.options.shadows;
        let (sun_view_proj, texel) = self
            .sun_view(frame.view_proj, terrain)
            .unwrap_or((Mat4::IDENTITY, 0.0));
        let mut globals = Globals {
            view_proj: frame.view_proj.to_cols_array_2d(),
            camera_pos: frame.camera_pos.extend(1.0).to_array(),
            sun_dir: sun.extend(0.0).to_array(),
            map: [
                terrain.samples_per_tile as f32,
                terrain.max_height,
                CHUNK_TILES as f32,
                CHUNK_QUADS as f32,
            ],
            fog: [fog_color[0], fog_color[1], fog_color[2], 0.0025],
            misc: [terrain.water_level, frame.time_seconds, 0.0, 0.0],
            sun_view_proj: sun_view_proj.to_cols_array_2d(),
            shadow: [
                texel * 1.5,
                if shadows { 1.0 } else { 0.0 },
                shadows::SHADOW_FADE,
                0.0,
            ],
        };
        // With nothing to draw the frame is a bare clear: no uploads and no
        // depth buffer, to measure the floor cost of presenting.
        let bare = !self.options.draw_terrain && !self.options.draw_blocks;
        if !bare {
            self.queue
                .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
            if shadows {
                // The shadow pass draws the world from the sun.
                globals.view_proj = globals.sun_view_proj;
                self.queue
                    .write_buffer(&self.shadow_globals, 0, bytemuck::bytes_of(&globals));
            }
        }

        // Pick each chunk's level of detail from its distance to the eye,
        // then frustum-cull on the CPU and upload only the visible chunks:
        // those the camera sees, then those the sun sees.
        let planes = frustum_planes(frame.view_proj);
        let sun_planes = frustum_planes(sun_view_proj);
        let n = terrain.chunks_per_side;
        let size = CHUNK_TILES as f32;
        for cz in 0..n {
            for cx in 0..n {
                self.chunk_levels[(cz * n + cx) as usize] =
                    chunk_level(frame.camera_pos, [cx, cz], terrain.max_height) as u8;
            }
        }
        // The camera's chunks fill the instance buffer's first half, the
        // sun's the second.
        self.visible_chunks.clear();
        self.sun_chunks.clear();
        for cz in 0..n {
            for cx in 0..n {
                let min = Vec3::new(cx as f32 * size, 0.0, cz as f32 * size);
                let max = Vec3::new(min.x + size, terrain.max_height, min.z + size);
                let camera_sees = aabb_visible(&planes, min, max);
                let sun_sees = shadows && aabb_visible(&sun_planes, min, max);
                if camera_sees || sun_sees {
                    let level = self.chunk_levels[(cz * n + cx) as usize] as usize;
                    let instance = chunk_instance(&self.chunk_levels, n, [cx, cz]);
                    if camera_sees {
                        self.visible_chunks.levels[level].push(instance);
                    }
                    if sun_sees {
                        self.sun_chunks.levels[level].push(instance);
                    }
                }
            }
        }
        debug_assert!(self.visible_chunks.len() <= self.chunk_instance_capacity as usize);
        let sun_chunks_at = u64::from(self.chunk_instance_capacity) * CHUNK_INSTANCE_BYTES;
        for (chunks, mut at) in [(&self.visible_chunks, 0), (&self.sun_chunks, sun_chunks_at)] {
            for instances in &chunks.levels {
                if !instances.is_empty() {
                    self.queue.write_buffer(
                        &self.chunk_instances,
                        at,
                        bytemuck::cast_slice(instances),
                    );
                    at += instances.len() as u64 * CHUNK_INSTANCE_BYTES;
                }
            }
        }
        if !self.options.draw_terrain {
            self.visible_chunks.clear();
            self.sun_chunks.clear();
        }
        let mut dropped = 0;
        let boxes = if self.options.draw_blocks {
            dropped += boxes.len().saturating_sub(MAX_BOX_INSTANCES);
            &boxes[..boxes.len().min(MAX_BOX_INSTANCES)]
        } else {
            &[]
        };
        if !boxes.is_empty() {
            self.queue
                .write_buffer(&self.box_instances, 0, bytemuck::cast_slice(boxes));
        }
        let soldiers = match &self.soldiers.models {
            Some(_) if self.options.draw_blocks => {
                dropped += soldiers.len().saturating_sub(MAX_SOLDIER_INSTANCES);
                &soldiers[..soldiers.len().min(MAX_SOLDIER_INSTANCES)]
            }
            _ => &[],
        };
        if !soldiers.is_empty() {
            self.queue
                .write_buffer(&self.soldiers.instances, 0, bytemuck::cast_slice(soldiers));
        }
        // Which soldiers are drawn at which level of detail, and where the
        // skinning pass puts their vertices.
        dropped += self
            .soldiers
            .prepare(&self.device, &self.queue, soldiers.len(), batches);
        let (mut soldiers_drawn, mut triangles, mut scenery_drawn) = (0, 0, 0);

        // The overlay draws in the composite pass, which a bare frame skips.
        let overlay_visible = !bare && self.overlay.count > 0;
        let ui_visible = !bare && self.ui.count > 0;
        for (gpu, visible) in [(&self.overlay, overlay_visible), (&self.ui, ui_visible)] {
            if visible {
                let screen = [
                    self.config.width as f32,
                    self.config.height as f32,
                    gpu.scale,
                    0.0,
                ];
                self.queue
                    .write_buffer(&gpu.globals, 0, bytemuck::bytes_of(&screen));
            }
        }

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let mut draw_calls = 0;
        let draw_boxes = |pass: &mut wgpu::RenderPass<'_>, pipeline| {
            if boxes.is_empty() {
                return 0;
            }
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, self.box_instances.slice(..));
            pass.draw(0..30, 0..boxes.len() as u32);
            1
        };
        // One instanced draw per level of detail that has chunks; each
        // level's chunks sit together in the instance buffer, finest first.
        let draw_terrain = |pass: &mut wgpu::RenderPass<'_>, shadow: bool| {
            let (pipeline, chunks, at) = if shadow {
                (
                    &self.terrain_shadow_pipeline,
                    &self.sun_chunks,
                    sun_chunks_at,
                )
            } else {
                (&self.terrain_pipeline, &self.visible_chunks, 0)
            };
            if chunks.is_empty() {
                return 0;
            }
            pass.set_pipeline(pipeline);
            pass.set_index_buffer(self.chunk_indices.slice(..), wgpu::IndexFormat::Uint16);
            pass.set_vertex_buffer(0, self.chunk_instances.slice(at..));
            let mut first = 0;
            let mut calls = 0;
            for (level, instances) in chunks.levels.iter().enumerate() {
                let count = instances.len() as u32;
                if count > 0 {
                    pass.draw_indexed(level_indices(level), 0, first..first + count);
                    calls += 1;
                }
                first += count;
            }
            calls
        };
        // Pose the soldiers once for both passes below.
        if !bare {
            self.soldiers.skin(&mut encoder, &terrain.bind_group);
        }
        if shadows && !bare {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &terrain.shadow_bind_group, &[]);
            draw_calls += draw_terrain(&mut pass, true);
            if self.options.draw_blocks {
                draw_calls += self.scenery.draw(&mut pass, true).0;
            }
            draw_calls += self.soldiers.draw(&mut pass, true).0;
            draw_calls += draw_boxes(&mut pass, &self.block_shadow_pipeline);
        }
        if !bare {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post_targets.hdr,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(fog_color[0]),
                            g: f64::from(fog_color[1]),
                            b: f64::from(fog_color[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &terrain.bind_group, &[]);
            draw_calls += draw_terrain(&mut pass, false);
            let (calls, scenery_instances, scenery_tris) = if self.options.draw_blocks {
                self.scenery.draw(&mut pass, false)
            } else {
                (0, 0, 0)
            };
            draw_calls += calls;
            scenery_drawn = scenery_instances;
            let (calls, drawn, tris) = self.soldiers.draw(&mut pass, false);
            draw_calls += calls;
            soldiers_drawn = drawn;
            triangles = tris + scenery_tris;
            draw_calls += draw_boxes(&mut pass, &self.block_pipeline);
            drop(pass);
            draw_calls += self.post.glow(&mut encoder, &self.post_targets);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("screen"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // A bare frame shows the sky colour; others cover
                        // every pixel.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: srgb_to_linear(SKY[0]),
                            g: srgb_to_linear(SKY[1]),
                            b: srgb_to_linear(SKY[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !bare {
                pass.set_pipeline(&self.post.composite);
                pass.set_bind_group(0, &self.post_targets.composite_group, &[]);
                pass.draw(0..3, 0..1);
                draw_calls += 1;
            }
            if ui_visible {
                pass.set_pipeline(&self.ui.pipeline);
                pass.set_bind_group(0, &self.ui.bind_group, &[]);
                pass.set_vertex_buffer(0, self.ui.instances.slice(..));
                pass.draw(0..4, 0..self.ui.count);
                draw_calls += 1;
            }
            if overlay_visible {
                pass.set_pipeline(&self.overlay.pipeline);
                pass.set_bind_group(0, &self.overlay.bind_group, &[]);
                pass.set_vertex_buffer(0, self.overlay.instances.slice(..));
                pass.draw(0..4, 0..self.overlay.count);
                draw_calls += 1;
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(surface_texture);
        if reconfigure {
            self.surface.configure(&self.device, &self.config);
        }

        self.stats = RenderStats {
            draw_calls,
            chunks_drawn: self.visible_chunks.len() as u32,
            chunks_total: n * n,
            boxes_drawn: boxes.len() as u32,
            soldiers_drawn,
            scenery_drawn,
            triangles,
            dropped: dropped as u32 + self.scenery.dropped(),
        };
        FrameOutcome::Presented
    }
}

fn scaled(width: u32, height: u32, scale: f32) -> (u32, u32) {
    let s = |v: u32| ((v as f32 * scale).round() as u32).max(1);
    (s(width), s(height))
}

/// What a world pipeline draws into.
#[derive(Clone, Copy)]
pub(crate) enum Target<'a> {
    /// The HDR image (in this format), with this fragment entry point.
    Color(wgpu::TextureFormat, &'a str),
    /// The sun's shadow map: depth only, pushed away from the sun.
    Shadow,
}

impl<'a> Target<'a> {
    pub(crate) fn depth_state(self) -> wgpu::DepthStencilState {
        let shadow = matches!(self, Target::Shadow);
        wgpu::DepthStencilState {
            format: if shadow { SHADOW_FORMAT } else { DEPTH_FORMAT },
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: if shadow {
                SHADOW_BIAS
            } else {
                Default::default()
            },
        }
    }

    /// The colour target (none for the shadow map) and fragment entry point.
    pub(crate) fn color(self) -> ([Option<wgpu::ColorTargetState>; 1], Option<&'a str>) {
        match self {
            Target::Color(format, fs) => (
                [Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                Some(fs),
            ),
            Target::Shadow => ([None], None),
        }
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    vs: &str,
    target: Target<'_>,
    instance_layout: wgpu::VertexBufferLayout<'_>,
) -> wgpu::RenderPipeline {
    let (targets, fs) = target.color();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[Some(instance_layout)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
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
}

fn make_overlay(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    capacity: usize,
) -> OverlayGpu {
    let font_size = wgpu::Extent3d {
        width: overlay::font_width(),
        height: debug_font::GLYPH_HEIGHT,
        depth_or_array_layers: 1,
    };
    let font = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("overlay font"),
        size: font_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texels = overlay::font_texels();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &font,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(font_size.width),
            rows_per_image: Some(font_size.height),
        },
        font_size,
    );
    let globals = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("overlay globals"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let instances = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("overlay glyphs"),
        size: (capacity * size_of::<UiQuad>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("overlay"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let font_view = font.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("overlay"),
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&font_view),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("overlay"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("overlay.wgsl"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/overlay.wgsl").into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("overlay"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_glyph"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<UiQuad>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Uint16x2, 1 => Uint16x2, 2 => Uint32, 3 => Unorm8x4
                ],
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            cull_mode: None,
            ..Default::default()
        },
        // Drawn over the finished frame, in the screen pass.
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_glyph"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    OverlayGpu {
        pipeline,
        bind_group,
        gpu_bytes: texels.len() as u64 + globals.size() + instances.size(),
        globals,
        instances,
        glyphs: Vec::with_capacity(if capacity == MAX_OVERLAY_CHARS {
            capacity
        } else {
            0
        }),
        count: 0,
        scale: 1.0,
    }
}

fn make_depth(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    transient_saves_memory: bool,
) -> (wgpu::TextureView, u64) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        // Cleared and discarded every pass, so on Apple GPUs it can live in
        // on-chip tile memory only (zero bytes of RAM). A no-op elsewhere.
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TRANSIENT_ATTACHMENT,
        view_formats: &[],
    });
    let bytes = if transient_saves_memory {
        0
    } else {
        u64::from(config.width) * u64::from(config.height) * 4
    };
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        bytes,
    )
}

/// Triangle-list indices for a `quads × quads` grid of `(quads+1)²` vertices.
fn chunk_grid_indices(quads: u32) -> Vec<u16> {
    let side = quads + 1;
    assert!(side * side <= u32::from(u16::MAX) + 1);
    let mut out = Vec::with_capacity((quads * quads * 6) as usize);
    for y in 0..quads {
        for x in 0..quads {
            let i = (y * side + x) as u16;
            let right = i + 1;
            let down = i + side as u16;
            let diag = down + 1;
            out.extend_from_slice(&[i, down, right, right, down, diag]);
        }
    }
    out
}

/// Where level `level`'s triangles sit in the shared index buffer: the
/// levels are stored finest first, one after the other.
fn level_indices(level: usize) -> std::ops::Range<u32> {
    let len = |l: usize| {
        let quads = CHUNK_QUADS >> l;
        quads * quads * 6
    };
    let start: u32 = (0..level).map(len).sum();
    start..start + len(level)
}

/// The level of detail for a chunk with these coordinates, seen from `eye`:
/// by the distance to the nearest point of the chunk's box, so a big chunk
/// is as fine as its nearest corner needs.
fn chunk_level(eye: Vec3, chunk: [u32; 2], max_height: f32) -> usize {
    let size = CHUNK_TILES as f32;
    let min = Vec3::new(chunk[0] as f32 * size, 0.0, chunk[1] as f32 * size);
    let max = Vec3::new(min.x + size, max_height, min.z + size);
    let distance = eye.clamp(min, max).distance(eye);
    LOD_DISTANCES
        .iter()
        .position(|&limit| distance < limit)
        .unwrap_or(LOD_DISTANCES.len())
}

/// What the terrain shader needs to know about one chunk: its coordinates,
/// then its level (bits 0-1) and its west, east, north and south
/// neighbours' levels (bits 2-3, 4-5, 6-7, 8-9) packed into one word. A
/// side on the map's edge has no neighbour and counts as the chunk's own
/// level, so its edge is left alone.
fn chunk_instance(levels: &[u8], chunks_per_side: u32, chunk: [u32; 2]) -> [u32; 3] {
    let n = chunks_per_side;
    let at = |x: u32, z: u32| u32::from(levels[(z * n + x) as usize]);
    let [x, z] = chunk;
    let own = at(x, z);
    let west = if x > 0 { at(x - 1, z) } else { own };
    let east = if x + 1 < n { at(x + 1, z) } else { own };
    let north = if z > 0 { at(x, z - 1) } else { own };
    let south = if z + 1 < n { at(x, z + 1) } else { own };
    [x, z, own | west << 2 | east << 4 | north << 6 | south << 8]
}

/// The chunks one pass draws, grouped by level of detail so that each
/// level is one instanced draw.
#[derive(Default)]
struct ChunkList {
    levels: [Vec<[u32; 3]>; TERRAIN_LEVELS],
}

impl ChunkList {
    fn with_capacity(chunks: usize) -> ChunkList {
        ChunkList {
            levels: std::array::from_fn(|_| Vec::with_capacity(chunks)),
        }
    }

    fn clear(&mut self) {
        self.levels.iter_mut().for_each(Vec::clear);
    }

    fn len(&self) -> usize {
        self.levels.iter().map(Vec::len).sum()
    }

    fn is_empty(&self) -> bool {
        self.levels.iter().all(Vec::is_empty)
    }
}

fn frustum_planes(view_proj: Mat4) -> [Vec4; 6] {
    let r0 = view_proj.row(0);
    let r1 = view_proj.row(1);
    let r2 = view_proj.row(2);
    let r3 = view_proj.row(3);
    // wgpu clip space: x,y in [-w, w], z in [0, w].
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
}

/// `planes` scaled so each normal has length one, which makes the distance
/// to a plane a distance in tiles.
fn unit_planes(planes: [Vec4; 6]) -> [Vec4; 6] {
    planes.map(|p| {
        let len = p.truncate().length();
        if len > 0.0 { p / len } else { p }
    })
}

fn aabb_visible(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> bool {
    planes.iter().all(|p| {
        // The box corner furthest along the plane normal.
        let v = Vec3::new(
            if p.x >= 0.0 { max.x } else { min.x },
            if p.y >= 0.0 { max.y } else { min.y },
            if p.z >= 0.0 { max.z } else { min.z },
        );
        p.truncate().dot(v) + p.w >= 0.0
    })
}

fn srgb_to_linear(c: f32) -> f64 {
    let c = f64::from(c);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_indices_cover_every_quad() {
        let idx = chunk_grid_indices(CHUNK_QUADS);
        assert_eq!(idx.len() as u32, CHUNK_QUADS * CHUNK_QUADS * 6);
        let max = u32::from(*idx.iter().max().unwrap());
        assert_eq!(max, (CHUNK_QUADS + 1) * (CHUNK_QUADS + 1) - 1);
    }

    #[test]
    fn each_level_has_a_quarter_of_the_triangles_and_its_own_index_range() {
        let mut end = 0;
        for level in 0..TERRAIN_LEVELS {
            let range = level_indices(level);
            assert_eq!(range.start, end);
            end = range.end;
            let quads = CHUNK_QUADS >> level;
            assert_eq!((range.end - range.start) / 3, quads * quads * 2);
            let indices = chunk_grid_indices(quads);
            assert_eq!(indices.len() as u32, range.end - range.start);
        }
    }

    #[test]
    fn chunks_get_coarser_with_distance_from_the_eye() {
        let height = 20.0;
        let eye = Vec3::new(0.0, 90.0, 0.0);
        assert_eq!(chunk_level(eye, [0, 0], height), 0);
        let levels: Vec<usize> = (0..24)
            .map(|cx| chunk_level(eye, [cx, 0], height))
            .collect();
        assert!(levels.windows(2).all(|w| w[0] <= w[1]), "{levels:?}");
        assert_eq!(*levels.first().unwrap(), 0);
        assert_eq!(*levels.last().unwrap(), TERRAIN_LEVELS - 1);
        // Neighbours are never more than one level apart.
        assert!(levels.windows(2).all(|w| w[1] - w[0] <= 1), "{levels:?}");
        // The distance is to the nearest point of the chunk, not its middle.
        // Its middle is 135 tiles away, past level 0's 128; its edge is 120.
        let beside = Vec3::new(31.0 + 120.0, 1.0, 16.0);
        assert_eq!(chunk_level(beside, [0, 0], height), 0);
    }

    #[test]
    fn chunk_instances_name_their_neighbours_levels() {
        // A 3 x 3 grid, levels by row: 0 0 1 / 1 2 3 / 3 3 3.
        let levels = [0, 0, 1, 1, 2, 3, 3, 3, 3];
        let unpack = |w: u32| [w & 3, w >> 2 & 3, w >> 4 & 3, w >> 6 & 3, w >> 8 & 3];
        // The middle chunk: own 2; west 1, east 3, north 0, south 3.
        let [x, z, word] = chunk_instance(&levels, 3, [1, 1]);
        assert_eq!([x, z], [1, 1]);
        assert_eq!(unpack(word), [2, 1, 3, 0, 3]);
        // The corner chunk has no west or north, which count as its own level.
        let [_, _, word] = chunk_instance(&levels, 3, [0, 0]);
        assert_eq!(unpack(word), [0, 0, 0, 0, 1]);
        let [_, _, word] = chunk_instance(&levels, 3, [2, 2]);
        assert_eq!(unpack(word), [3, 3, 3, 3, 3]);
    }

    /// The terrain vertex shader's edge rule, in Rust: where one chunk's
    /// vertices land along an edge it shares with a neighbour.
    fn edge_positions(level: u32, neighbour: u32) -> Vec<u32> {
        let quads = CHUNK_QUADS >> level;
        (0..=quads)
            .map(|i| {
                let p = i << level;
                if neighbour > level {
                    (p >> neighbour) << neighbour
                } else {
                    p
                }
            })
            .collect()
    }

    #[test]
    fn a_finer_chunks_edge_lands_on_the_coarser_chunks_vertices() {
        for coarse in 1..TERRAIN_LEVELS as u32 {
            let theirs = edge_positions(coarse, 0);
            for fine in 0..coarse {
                let ours = edge_positions(fine, coarse);
                // Every vertex drawn on the edge is one of the neighbour's,
                // so the two edges are the same polyline.
                assert!(ours.iter().all(|p| theirs.contains(p)), "{fine} {coarse}");
                // Both run corner to corner, in order.
                assert_eq!((ours[0], *ours.last().unwrap()), (0, CHUNK_QUADS));
                assert!(ours.windows(2).all(|w| w[0] <= w[1]));
            }
        }
    }

    #[test]
    fn frustum_culls_boxes_behind_camera() {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let view = look_at_mat4(
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::Y,
        );
        let proj = perspective(1.0, 1.5, 0.5, 500.0);
        let planes = frustum_planes(proj * view);
        assert!(aabb_visible(
            &planes,
            Vec3::new(-5.0, 0.0, -30.0),
            Vec3::new(5.0, 2.0, -20.0)
        ));
        assert!(!aabb_visible(
            &planes,
            Vec3::new(-5.0, 0.0, 20.0),
            Vec3::new(5.0, 2.0, 30.0)
        ));
    }

    #[test]
    fn soldiers_off_screen_are_culled_unless_the_sun_sees_them() {
        use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
        let view = look_at_mat4(
            Vec3::new(0.0, 10.0, 0.0),
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::Y,
        );
        let proj = perspective(1.0, 1.5, 0.5, 500.0);
        let on_screen = Vec3::new(0.0, 1.0, -25.0);
        let behind = Vec3::new(0.0, 1.0, 25.0);
        let cull = Cull::new(proj * view, None);
        assert!(cull.sees(on_screen, 1.0));
        assert!(!cull.sees(behind, 1.0));
        // A soldier's size counts: its edge reaches the screen.
        let beside = Vec3::new(-30.0, 1.0, -10.0);
        assert!(!cull.sees(beside, 1.0));
        assert!(cull.sees(beside, 20.0));
        // Behind the camera but inside the sun's box: still drawn, for the
        // shadow it casts.
        let sun_view = look_at_mat4(Vec3::new(0.0, 50.0, 30.0), Vec3::ZERO, Vec3::Y);
        let sun_proj =
            glam::camera::rh::proj::directx::orthographic(-40.0, 40.0, -40.0, 40.0, 0.0, 200.0);
        let cull = Cull::new(proj * view, Some(sun_proj * sun_view));
        assert!(cull.sees(behind, 1.0));
        assert!(Cull::EVERYTHING.sees(behind, 0.0));
        assert!(Cull::new(Mat4::from_cols_array(&[f32::NAN; 16]), None).sees(behind, 0.0));
    }

    #[test]
    fn shaders_validate() {
        for src in [
            include_str!("shaders/world.wgsl"),
            include_str!("shaders/overlay.wgsl"),
            include_str!("shaders/post.wgsl"),
            scenery::shader_source().as_str(),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        }
    }

    #[test]
    fn overlay_instance_layout_is_stable() {
        assert_eq!(size_of::<UiQuad>(), 16);
    }

    #[test]
    fn regiment_instance_layout_is_stable() {
        assert_eq!(size_of::<BoxInstance>(), 36);
        assert_eq!(size_of::<SoldierInstance>(), 36);
    }

    #[test]
    fn a_repeating_gpu_error_is_reported_once() {
        let health = GpuHealth::default();
        let take = |h: &GpuHealth| std::mem::take(&mut h.messages.lock().unwrap().pending);
        for _ in 0..1000 {
            health.report("bad bind group".into());
        }
        assert_eq!(take(&health).len(), 1);
        health.report("something else".into());
        health.report("bad bind group".into());
        assert_eq!(take(&health).len(), 1);
        // Endless different messages are bounded too.
        for n in 0..1000 {
            health.report(format!("error {n}"));
        }
        let m = health.messages.lock().unwrap();
        assert!(m.seen.len() <= MAX_GPU_DISTINCT);
        assert!(m.pending.len() <= MAX_GPU_MESSAGES);
    }
}
