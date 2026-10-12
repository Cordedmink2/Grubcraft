//! Grubcraft renderer: wgpu on Metal (macOS), DirectX 12 (Windows) and Vulkan.
//!
//! M0 scope: heightmap terrain drawn in frustum-culled chunks, and regiments
//! drawn as instanced blocks. See `docs/10-architecture.md#rendering` for where
//! this is heading (shadows, vertex-animated soldiers, Commander View, UI).
//!
//! The renderer knows nothing about windows or input: the caller hands it a
//! surface target and per-frame camera and instance data.

use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};

/// Tiles per terrain chunk side (chunks are the unit of culling).
pub const CHUNK_TILES: u32 = 32;
/// Quads per chunk side; vertices sit every half tile.
const CHUNK_QUADS: u32 = 64;
/// Fixed capacity of the regiment instance buffer; never reallocated.
pub const MAX_REGIMENT_INSTANCES: usize = 2048;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug)]
pub enum InitError {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::Surface(e) => write!(f, "could not create a drawing surface: {e}"),
            InitError::Adapter(e) => write!(f, "no suitable GPU found: {e}"),
            InitError::Device(e) => write!(f, "could not open the GPU: {e}"),
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
}

/// One regiment block. 32 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct RegimentInstance {
    /// Centre on the map, in tiles.
    pub pos: [f32; 2],
    /// Unit vector the regiment faces.
    pub facing: [f32; 2],
    /// Frontage width, height, depth in tiles.
    pub size: [f32; 3],
    /// Team colour (rgb) and selection highlight (a).
    pub color: [u8; 4],
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
}

impl Default for RendererOptions {
    fn default() -> Self {
        RendererOptions {
            frame_latency: 2,
            render_scale: 1.0,
            draw_terrain: true,
            draw_blocks: true,
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
    pub regiments_drawn: u32,
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
}

struct TerrainGpu {
    bind_group: wgpu::BindGroup,
    chunks_per_side: u32,
    max_height: f32,
    samples_per_tile: u32,
    water_level: f32,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    options: RendererOptions,
    depth_view: wgpu::TextureView,
    depth_bytes: u64,
    globals: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    terrain_pipeline: wgpu::RenderPipeline,
    block_pipeline: wgpu::RenderPipeline,
    chunk_index_buffer: wgpu::Buffer,
    chunk_index_count: u32,
    chunk_instances: wgpu::Buffer,
    chunk_instance_capacity: u32,
    regiment_instances: wgpu::Buffer,
    terrain: Option<TerrainGpu>,
    /// GPU memory we allocated ourselves (excludes the swapchain and driver).
    static_gpu_bytes: u64,
    terrain_gpu_bytes: u64,
    stats: RenderStats,
    adapter_info: wgpu::AdapterInfo,
    /// Scratch list of visible chunk coordinates, reused every frame.
    visible_chunks: Vec<[u32; 2]>,
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("grubcraft"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .map_err(InitError::Device)?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
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

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(size_of::<Globals>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
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

        let terrain_pipeline = make_pipeline(
            &device,
            &layout,
            &shader,
            format,
            "terrain",
            ("vs_terrain", "fs_terrain"),
            wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Uint32x2],
            },
        );
        let block_pipeline = make_pipeline(
            &device,
            &layout,
            &shader,
            format,
            "regiment blocks",
            ("vs_block", "fs_block"),
            wgpu::VertexBufferLayout {
                array_stride: size_of::<RegimentInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32x3, 3 => Unorm8x4
                ],
            },
        );

        let chunk_indices = chunk_grid_indices(CHUNK_QUADS);
        let chunk_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk grid indices"),
            size: (chunk_indices.len() * 2) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&chunk_index_buffer, 0, bytemuck::cast_slice(&chunk_indices));

        let regiment_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("regiment instances"),
            size: (MAX_REGIMENT_INSTANCES * size_of::<RegimentInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Placeholder until terrain arrives; replaced by `set_terrain`.
        let chunk_instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk instances"),
            size: 8,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let adapter_info = adapter.get_info();
        let (depth_view, depth_bytes) = make_depth(
            &device,
            &config,
            adapter_info.transient_saves_memory == Some(true),
        );
        let static_gpu_bytes = size_of::<Globals>() as u64
            + chunk_index_buffer.size()
            + regiment_instances.size()
            + chunk_instances.size();

        Ok(Renderer {
            surface,
            device,
            queue,
            config,
            options,
            depth_view,
            depth_bytes,
            globals,
            bind_group_layout,
            terrain_pipeline,
            block_pipeline,
            chunk_index_buffer,
            chunk_index_count: chunk_indices.len() as u32,
            chunk_instances,
            chunk_instance_capacity: 1,
            regiment_instances,
            terrain: None,
            static_gpu_bytes,
            terrain_gpu_bytes: 0,
            stats: RenderStats::default(),
            adapter_info,
            visible_chunks: Vec::new(),
        })
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// GPU memory allocated by the renderer itself, in bytes.
    pub fn gpu_bytes(&self) -> u64 {
        self.static_gpu_bytes + self.terrain_gpu_bytes + self.depth_bytes
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
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });

        let tiles = (t.size - 1) / t.samples_per_tile;
        let chunks_per_side = tiles.div_ceil(CHUNK_TILES);
        let chunk_count = chunks_per_side * chunks_per_side;
        self.static_gpu_bytes -= self.chunk_instances.size();
        self.chunk_instances = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk instances"),
            size: chunk_count as u64 * 8,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.static_gpu_bytes += self.chunk_instances.size();
        self.chunk_instance_capacity = chunk_count;
        self.visible_chunks = Vec::with_capacity(chunk_count as usize);
        self.terrain_gpu_bytes = t.heights.len() as u64 * 2;
        self.terrain = Some(TerrainGpu {
            bind_group,
            chunks_per_side,
            max_height: t.max_height,
            samples_per_tile: t.samples_per_tile,
            water_level: t.water_level,
        });
    }

    pub fn render(&mut self, frame: &FrameParams, regiments: &[RegimentInstance]) -> FrameOutcome {
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

        let fog_color = [0.62f32, 0.66, 0.70];
        let sun = Vec3::new(-0.45, 0.75, -0.35).normalize();
        let globals = Globals {
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
        };
        // With nothing to draw the frame is a bare clear: no uploads and no
        // depth buffer, to measure the floor cost of presenting.
        let bare = !self.options.draw_terrain && !self.options.draw_blocks;
        if !bare {
            self.queue
                .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        }

        // Frustum-cull terrain chunks on the CPU; upload only the visible ones.
        let planes = frustum_planes(frame.view_proj);
        self.visible_chunks.clear();
        let n = terrain.chunks_per_side;
        let size = CHUNK_TILES as f32;
        for cz in 0..n {
            for cx in 0..n {
                let min = Vec3::new(cx as f32 * size, 0.0, cz as f32 * size);
                let max = Vec3::new(min.x + size, terrain.max_height, min.z + size);
                if aabb_visible(&planes, min, max) {
                    self.visible_chunks.push([cx, cz]);
                }
            }
        }
        debug_assert!(self.visible_chunks.len() <= self.chunk_instance_capacity as usize);
        if !self.visible_chunks.is_empty() {
            self.queue.write_buffer(
                &self.chunk_instances,
                0,
                bytemuck::cast_slice(&self.visible_chunks),
            );
        }
        if !self.options.draw_terrain {
            self.visible_chunks.clear();
        }
        let regiments = if self.options.draw_blocks {
            &regiments[..regiments.len().min(MAX_REGIMENT_INSTANCES)]
        } else {
            &[]
        };
        if !regiments.is_empty() {
            self.queue
                .write_buffer(&self.regiment_instances, 0, bytemuck::cast_slice(regiments));
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
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: srgb_to_linear(fog_color[0]),
                            g: srgb_to_linear(fog_color[1]),
                            b: srgb_to_linear(fog_color[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: (!bare).then_some(
                    wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    },
                ),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !bare {
                pass.set_bind_group(0, &terrain.bind_group, &[]);
            }

            if !self.visible_chunks.is_empty() {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_index_buffer(self.chunk_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.set_vertex_buffer(0, self.chunk_instances.slice(..));
                pass.draw_indexed(
                    0..self.chunk_index_count,
                    0,
                    0..self.visible_chunks.len() as u32,
                );
                draw_calls += 1;
            }
            if !regiments.is_empty() {
                pass.set_pipeline(&self.block_pipeline);
                pass.set_vertex_buffer(0, self.regiment_instances.slice(..));
                pass.draw(0..30, 0..regiments.len() as u32);
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
            regiments_drawn: regiments.len() as u32,
        };
        FrameOutcome::Presented
    }
}

fn scaled(width: u32, height: u32, scale: f32) -> (u32, u32) {
    let s = |v: u32| ((v as f32 * scale).round() as u32).max(1);
    (s(width), s(height))
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    label: &str,
    (vs, fs): (&str, &str),
    instance_layout: wgpu::VertexBufferLayout<'_>,
) -> wgpu::RenderPipeline {
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
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
        config.width as u64 * config.height as u64 * 4
    };
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        bytes,
    )
}

/// Triangle-list indices for a `quads × quads` grid of `(quads+1)²` vertices.
fn chunk_grid_indices(quads: u32) -> Vec<u16> {
    let side = quads + 1;
    assert!(side * side <= u16::MAX as u32 + 1);
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

fn frustum_planes(view_proj: Mat4) -> [Vec4; 6] {
    let r0 = view_proj.row(0);
    let r1 = view_proj.row(1);
    let r2 = view_proj.row(2);
    let r3 = view_proj.row(3);
    // wgpu clip space: x,y in [-w, w], z in [0, w].
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
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
    let c = c as f64;
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
        let max = *idx.iter().max().unwrap() as u32;
        assert_eq!(max, (CHUNK_QUADS + 1) * (CHUNK_QUADS + 1) - 1);
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
    fn shaders_validate() {
        let src = include_str!("shaders/world.wgsl");
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    }

    #[test]
    fn regiment_instance_layout_is_stable() {
        assert_eq!(size_of::<RegimentInstance>(), 32);
    }
}
