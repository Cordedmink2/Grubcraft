//! Soldier models on the GPU: every model's vertices and indices in one
//! buffer each, the bone animation texture, the material texture array, and
//! one instanced draw per model and level of detail.

use bytemuck::{Pod, Zeroable};
use gc_assets::model::{
    BONE_TEXELS_PER_ROW, LAYERS_PER_MATERIAL, ModelVertex, Models, TEXELS_PER_LAYER, TEXTURE_MIPS,
    TEXTURE_SIZE,
};

/// Fixed capacity of the soldier instance buffer; never reallocated.
pub const MAX_SOLDIER_INSTANCES: usize = 65_536;

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
    /// 0 for full detail, 1 for the reduced mesh.
    pub lod: u8,
    pub first: u32,
    pub count: u32,
}

pub(crate) struct ModelGpu {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    /// Per model: first vertex, (first index, count) per level of detail,
    /// and whether it is textured.
    pub ranges: Vec<(i32, [[u32; 2]; 2], bool)>,
    pub gpu_bytes: u64,
}

pub(crate) struct SoldiersGpu {
    /// For models painted in vertex colours.
    pub pipeline: wgpu::RenderPipeline,
    pub textured_pipeline: wgpu::RenderPipeline,
    /// Depth only, for the sun's shadow map; any model.
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub bones_layout: wgpu::BindGroupLayout,
    pub instances: wgpu::Buffer,
    pub models: Option<ModelGpu>,
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
    let bones_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("soldier models"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            // The material array seen as sRGB (colour layers) and as linear
            // (the normal layer).
            materials(1),
            materials(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("soldiers"),
        bind_group_layouts: &[Some(world_layout), Some(&bones_layout)],
        immediate_size: 0,
    });
    let pipeline = |label, vs, target: crate::Target<'_>, cull_mode| {
        let (targets, fs) = target.color();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<ModelVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        // Position and team weight, bones, weights, normal,
                        // tangent, colour, texture coordinate.
                        attributes: &wgpu::vertex_attr_array![
                            0 => Sint16x4, 1 => Uint8x4, 2 => Unorm8x4, 3 => Snorm8x4,
                            4 => Snorm8x4, 5 => Unorm8x4, 6 => Unorm16x2
                        ],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<SoldierInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            7 => Float32x2, 8 => Float32x2, 9 => Uint16x2, 10 => Unorm8x4,
                            11 => Unorm8x4, 12 => Float32, 13 => Float32
                        ],
                    }),
                ],
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
    // The low-poly packs model cloaks and blades as single faces; textured
    // models are built closed, so their back faces can be skipped.
    let textured_pipeline = pipeline(
        "textured soldiers",
        "vs_soldier_textured",
        crate::Target::Color(format, "fs_soldier_textured"),
        Some(wgpu::Face::Back),
    );
    let shadow_pipeline = pipeline(
        "soldier shadows",
        "vs_soldier_depth",
        crate::Target::Shadow,
        None,
    );
    let pipeline = pipeline(
        "soldiers",
        "vs_soldier",
        crate::Target::Color(format, "fs_soldier"),
        None,
    );
    let instances = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("soldier instances"),
        size: (MAX_SOLDIER_INSTANCES * size_of::<SoldierInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    SoldiersGpu {
        pipeline,
        textured_pipeline,
        shadow_pipeline,
        bones_layout,
        instances,
        models: None,
    }
}

impl SoldiersGpu {
    /// Draw the first `count` uploaded instances by `batches`, each at its
    /// level of detail, into the frame or (with `shadow`) the shadow map.
    /// Returns draw calls, soldiers and triangles drawn.
    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        count: usize,
        batches: &[SoldierBatch],
        shadow: bool,
    ) -> (u32, u32, u64) {
        let (mut calls, mut drawn, mut tris) = (0, 0, 0);
        let Some(models) = self.models.as_ref().filter(|_| count > 0) else {
            return (calls, drawn, tris);
        };
        pass.set_bind_group(1, &models.bind_group, &[]);
        pass.set_vertex_buffer(0, models.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(models.indices.slice(..), wgpu::IndexFormat::Uint16);
        if shadow {
            pass.set_pipeline(&self.shadow_pipeline);
        }
        let mut textured_now = None;
        for b in batches {
            let Some(&(base, lods, textured)) = models.ranges.get(b.model as usize) else {
                continue;
            };
            if !shadow && textured_now != Some(textured) {
                pass.set_pipeline(if textured {
                    &self.textured_pipeline
                } else {
                    &self.pipeline
                });
                textured_now = Some(textured);
            }
            // A model without the reduced mesh draws its full one.
            let lod = (b.lod as usize).min(1);
            let [first, len] = if lods[lod][1] == 0 {
                lods[0]
            } else {
                lods[lod]
            };
            let end = (b.first + b.count).min(count as u32);
            if b.first >= end || len == 0 {
                continue;
            }
            pass.draw_indexed(first..first + len, base, b.first..end);
            calls += 1;
            drawn += end - b.first;
            tris += u64::from(len / 3) * u64::from(end - b.first);
        }
        (calls, drawn, tris)
    }
}

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bones_layout: &wgpu::BindGroupLayout,
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
    let vertices = buffer(
        "model vertices",
        bytemuck::cast_slice(m.vertices),
        wgpu::BufferUsages::VERTEX,
    );
    let indices = buffer(
        "model indices",
        bytemuck::cast_slice(m.indices),
        wgpu::BufferUsages::INDEX,
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
    let materials = upload_materials(device, queue, m);
    let array_view = |format| {
        materials.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    };
    let srgb_view = array_view(wgpu::TextureFormat::Rgba8UnormSrgb);
    let linear_view = array_view(wgpu::TextureFormat::Rgba8Unorm);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("soldier materials"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 4,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("soldier models"),
        layout: bones_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&srgb_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&linear_view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let ranges = m
        .models
        .iter()
        .map(|r| {
            let textured = m
                .vertices
                .get(r.first_vertex as usize)
                .is_some_and(|v| v.material > 0);
            (r.first_vertex as i32, r.lods, textured)
        })
        .collect();
    ModelGpu {
        gpu_bytes: vertices.size()
            + indices.size()
            + (texels.len() * 8) as u64
            + (m.textures.len() * 4) as u64,
        vertices,
        indices,
        bind_group,
        ranges,
    }
}

/// The material texture array, every layer with its mips; a single blank
/// texel when no model is textured, since the binding must exist.
fn upload_materials(device: &wgpu::Device, queue: &wgpu::Queue, m: &Models<'_>) -> wgpu::Texture {
    let layers = (m.materials() * LAYERS_PER_MATERIAL) as u32;
    let (side, mips) = if layers == 0 {
        (1, 1)
    } else {
        (TEXTURE_SIZE, TEXTURE_MIPS)
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("soldier materials"),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: layers.max(1),
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    if layers == 0 {
        return texture;
    }
    for (layer, texels) in m
        .textures
        .as_chunks::<TEXELS_PER_LAYER>()
        .0
        .iter()
        .enumerate()
    {
        let mut offset = 0;
        for mip in 0..TEXTURE_MIPS {
            let side = TEXTURE_SIZE >> mip;
            let count = (side * side) as usize;
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
                bytemuck::cast_slice(&texels[offset..offset + count]),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(side * 4),
                    rows_per_image: Some(side),
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
