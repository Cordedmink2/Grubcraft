//! Soldier models on the GPU: every model's vertices and indices in one
//! buffer each, the bone animation texture, and one instanced draw per model
//! and level of detail.

use bytemuck::{Pod, Zeroable};
use gc_assets::model::{BONE_TEXELS_PER_ROW, Models};

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
    /// Per model: first vertex, and (first index, count) per level of detail.
    pub ranges: Vec<(i32, [[u32; 2]; 2])>,
    pub gpu_bytes: u64,
}

pub(crate) struct SoldiersGpu {
    pub pipeline: wgpu::RenderPipeline,
    pub bones_layout: wgpu::BindGroupLayout,
    pub instances: wgpu::Buffer,
    pub models: Option<ModelGpu>,
}

pub(crate) fn make(
    device: &wgpu::Device,
    world_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    depth: wgpu::TextureFormat,
) -> SoldiersGpu {
    let bones_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bones"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("soldiers"),
        bind_group_layouts: &[Some(world_layout), Some(&bones_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("soldiers"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_soldier"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Sint16x4, 1 => Snorm8x4, 2 => Unorm8x4
                    ],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<SoldierInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        3 => Float32x2, 4 => Float32x2, 5 => Uint16x2, 6 => Unorm8x4,
                        7 => Unorm8x4, 8 => Float32, 9 => Float32
                    ],
                }),
            ],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // The low-poly packs model cloaks and blades as single faces.
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_soldier"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let instances = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("soldier instances"),
        size: (MAX_SOLDIER_INSTANCES * size_of::<SoldierInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    SoldiersGpu {
        pipeline,
        bones_layout,
        instances,
        models: None,
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bones"),
        layout: bones_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    let ranges = m
        .models
        .iter()
        .map(|r| (r.first_vertex as i32, r.lods))
        .collect();
    ModelGpu {
        gpu_bytes: vertices.size() + indices.size() + (texels.len() * 8) as u64,
        vertices,
        indices,
        bind_group,
        ranges,
    }
}
