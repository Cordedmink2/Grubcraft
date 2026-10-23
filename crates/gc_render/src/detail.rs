//! The shared detail textures (`gc_assets::detail`) on the GPU: one array of
//! BC7 layers, two per surface class, and a repeating sampler. Soldiers and
//! buildings layer them over their baked atlas (`fs_soldier_textured` in
//! `world.wgsl`); scenery can bind the same view.

use gc_assets::detail::{BLOCK_SIDE, DETAIL_MIPS, DETAIL_SIZE, Detail, LAYERS};
use gc_assets::model::Block;

/// The detail array and the sampler that reads it.
pub(crate) struct DetailGpu {
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    bytes: u64,
}

impl DetailGpu {
    /// Upload `detail`, when the GPU takes BC7 and enough array layers;
    /// otherwise (or with none) a texture of the same shape whose every
    /// texel leaves a surface as it is, since the binding must exist.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, detail: Option<&Detail<'_>>) -> Self {
        let real = detail.filter(|_| {
            device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
                && LAYERS <= device.limits().max_texture_array_layers as usize
        });
        let (format, side, mips) = if real.is_some() {
            (wgpu::TextureFormat::Bc7RgbaUnorm, DETAIL_SIZE, DETAIL_MIPS)
        } else {
            (wgpu::TextureFormat::Rgba8Unorm, 1, 1)
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("surface detail"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: LAYERS as u32,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        match real {
            Some(detail) => {
                upload_bc7_layers(queue, &texture, detail.textures, DETAIL_SIZE, DETAIL_MIPS);
            }
            None => {
                // Layer by layer: albedo neutral (0.5, cavity open), relief
                // flat (normal 0.5, roughness change 0.5, height 0.5).
                for layer in 0..LAYERS as u32 {
                    let texel: [u8; 4] = if layer % 2 == 0 {
                        [128, 128, 128, 255]
                    } else {
                        [128, 128, 128, 128]
                    };
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: 0,
                                y: 0,
                                z: layer,
                            },
                            aspect: wgpu::TextureAspect::All,
                        },
                        &texel,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4),
                            rows_per_image: Some(1),
                        },
                        wgpu::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("surface detail"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 2,
            ..Default::default()
        });
        let bytes = real.map_or(4 * LAYERS as u64, |d| size_of_val(d.textures) as u64);
        DetailGpu {
            view,
            sampler,
            bytes,
        }
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.bytes
    }
}

/// Upload BC7 `blocks`, as many blocks as one layer takes to each layer of `texture` (every
/// layer `side` texels square with `mips` mip levels, mip by mip).
pub(crate) fn upload_bc7_layers(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    blocks: &[Block],
    side: u32,
    mips: u32,
) {
    let per_layer: usize = (0..mips)
        .map(|mip| ((side >> mip) / BLOCK_SIDE).pow(2) as usize)
        .sum();
    debug_assert_eq!(blocks.len() % per_layer, 0, "blocks are not whole layers");
    for (layer, blocks) in blocks.chunks_exact(per_layer).enumerate() {
        let mut offset = 0;
        for mip in 0..mips {
            let side = side >> mip;
            let across = side / BLOCK_SIDE;
            let count = (across * across) as usize;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
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
}
