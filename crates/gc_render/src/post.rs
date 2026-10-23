//! The frame after the world is drawn: the HDR image the world renders
//! into, the glow (a chain of half-size images, see `shaders/post.wgsl`),
//! and the composite that grades both onto the screen.

use crate::look::{Look, rgba};

/// How strongly the glow adds to the frame.
const BLOOM_STRENGTH: f64 = 0.55;
/// Levels in the glow chain, at most; each is half the size of the last,
/// the first half the frame's.
const BLOOM_LEVELS: u32 = 6;

pub(crate) struct PostGpu {
    /// The format of the HDR image and the glow chain.
    hdr_format: wgpu::TextureFormat,
    chain_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    prefilter: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    pub composite: wgpu::RenderPipeline,
    glow: bool,
    /// The grade (see [`Grade`]), read by the composite.
    grade: wgpu::Buffer,
}

/// What the composite grades the frame with (`Grade` in `post.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Grade {
    /// Exposure, contrast, saturation, vignette.
    tone: [f32; 4],
    /// Tint for the dark end (rgb).
    shadow_tint: [f32; 4],
    /// Tint for the bright end (rgb).
    light_tint: [f32; 4],
    /// Film grain, time in seconds.
    film: [f32; 4],
}

/// Images sized to the frame, remade when the window resizes.
pub(crate) struct PostTargets {
    pub hdr: wgpu::TextureView,
    levels: Vec<Level>,
    hdr_group: wgpu::BindGroup,
    pub composite_group: wgpu::BindGroup,
    pub gpu_bytes: u64,
}

/// One image of the glow chain.
struct Level {
    /// To draw into.
    view: wgpu::TextureView,
    /// To read from.
    group: wgpu::BindGroup,
}

pub(crate) fn make(
    device: &wgpu::Device,
    hdr_format: wgpu::TextureFormat,
    screen_format: wgpu::TextureFormat,
    glow: bool,
) -> PostGpu {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let sampler_entry = wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    };
    let chain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("glow level"),
        entries: &[texture(0), sampler_entry],
    });
    let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("composite"),
        entries: &[
            texture(0),
            sampler_entry,
            texture(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("post.wgsl"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("post"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    // The glow chain's pipelines draw into the HDR format; `None` for
    // `chain` is the composite, drawing to the screen.
    let pipeline = |label, fs, chain: Option<Option<wgpu::BlendState>>| {
        let (layout, format, blend) = match chain {
            Some(blend) => (&chain_layout, hdr_format, blend),
            None => (&composite_layout, screen_format, None),
        };
        let strength = if glow { BLOOM_STRENGTH } else { 0.0 };
        let constants: &[(&str, f64)] = match chain {
            Some(_) => &[],
            None => &[("BLOOM_STRENGTH", strength)],
        };
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let add = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    let add = Some(wgpu::BlendState {
        color: add,
        alpha: add,
    });
    PostGpu {
        hdr_format,
        prefilter: pipeline("glow prefilter", "fs_bloom_prefilter", Some(None)),
        down: pipeline("glow down", "fs_bloom_down", Some(None)),
        up: pipeline("glow up", "fs_bloom_up", Some(add)),
        composite: pipeline("composite", "fs_composite", None),
        chain_layout,
        composite_layout,
        sampler,
        glow,
        grade: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grade"),
            size: size_of::<Grade>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
    }
}

impl PostGpu {
    /// Upload the grade of `look`, and the time for the film grain.
    pub fn write_grade(&self, queue: &wgpu::Queue, look: &Look, time_seconds: f32) {
        let grade = Grade {
            tone: [look.exposure, look.contrast, look.saturation, look.vignette],
            shadow_tint: rgba(look.shadow_tint, 0.0),
            light_tint: rgba(look.light_tint, 0.0),
            film: [look.grain, time_seconds, 0.0, 0.0],
        };
        queue.write_buffer(&self.grade, 0, bytemuck::bytes_of(&grade));
    }

    /// The HDR image and glow chain for a `width × height` frame.
    pub fn targets(&self, device: &wgpu::Device, width: u32, height: u32) -> PostTargets {
        let image = |label, width, height, mips| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.hdr_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let texel_bytes = u64::from(self.hdr_format.block_copy_size(None).unwrap_or(8));
        let hdr = image("hdr", width, height, 1);
        let hdr = hdr.create_view(&Default::default());
        let mut gpu_bytes = u64::from(width) * u64::from(height) * texel_bytes;
        let mut levels = Vec::new();
        let (w, h) = ((width / 2).max(1), (height / 2).max(1));
        if self.glow {
            let count = (w.min(h).max(1).ilog2().saturating_sub(2)).clamp(1, BLOOM_LEVELS);
            let chain = image("glow", w, h, count);
            for mip in 0..count {
                let view = chain.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                let group = self.group(device, &self.chain_layout, &view, None);
                levels.push(Level { view, group });
                gpu_bytes +=
                    u64::from((w >> mip).max(1)) * u64::from((h >> mip).max(1)) * texel_bytes;
            }
        }
        // With the glow off the composite never reads its glow binding.
        let glow_view = levels.first().map_or(&hdr, |l| &l.view);
        let composite_group = self.group(device, &self.composite_layout, &hdr, Some(glow_view));
        PostTargets {
            hdr_group: self.group(device, &self.chain_layout, &hdr, None),
            hdr,
            levels,
            composite_group,
            gpu_bytes,
        }
    }

    /// A bind group reading `view` (and, for the composite, `glow`).
    fn group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        glow: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ];
        if let Some(glow) = glow {
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(glow),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: self.grade.as_entire_binding(),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout,
            entries: &entries,
        })
    }

    /// Build the glow chain from the drawn HDR image. Returns the passes run.
    pub fn glow(&self, encoder: &mut wgpu::CommandEncoder, t: &PostTargets) -> u32 {
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    pipeline: &wgpu::RenderPipeline,
                    from: &wgpu::BindGroup,
                    to: &wgpu::TextureView,
                    load| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("glow"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: to,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, from, &[]);
            pass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let Some(first) = t.levels.first() else {
            return 0;
        };
        pass(encoder, &self.prefilter, &t.hdr_group, &first.view, clear);
        for pair in t.levels.windows(2) {
            let [big, small] = pair else { unreachable!() };
            pass(encoder, &self.down, &big.group, &small.view, clear);
        }
        for pair in t.levels.windows(2).rev() {
            let [big, small] = pair else { unreachable!() };
            pass(
                encoder,
                &self.up,
                &small.group,
                &big.view,
                wgpu::LoadOp::Load,
            );
        }
        (t.levels.len() * 2 - 1) as u32
    }
}
