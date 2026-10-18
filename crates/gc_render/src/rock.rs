//! The Rock035 textures `fs_terrain` paints mountains with. The PNGs under
//! `assets/textures/terrain` are compiled into the binary (like the shaders,
//! so no packaging step has to find them), decoded at start-up, given a
//! mip chain on the CPU and uploaded as two textures: albedo with roughness
//! in alpha, and the normal map. Together they are about 2.8 MB of GPU
//! memory with mips. If decoding fails the terrain falls back to noise-only
//! rock (`Globals::misc.z` is 0).

const ALBEDO: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_albedo.png");
const NORMAL: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_normal.png");
const ROUGH: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_rough.png");

/// The rock textures, or neutral 1 x 1 stand-ins.
pub struct RockTextures {
    pub color: wgpu::TextureView,
    pub normal: wgpu::TextureView,
    /// Whether the views hold real rock.
    pub real: bool,
    pub bytes: u64,
}

struct Image {
    side: u32,
    channels: usize,
    data: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Option<Image> {
    let mut reader = png::Decoder::new(bytes).read_info().ok()?;
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data).ok()?;
    if info.width != info.height
        || !info.width.is_power_of_two()
        || info.bit_depth != png::BitDepth::Eight
    {
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    data.truncate(info.buffer_size());
    Some(Image {
        side: info.width,
        channels,
        data,
    })
}

/// RGBA8 texels of `rgb` with `alpha` (a gray image) in the fourth channel.
fn pack(rgb: &Image, alpha: Option<&Image>) -> Option<Vec<u8>> {
    if rgb.channels < 3 || alpha.is_some_and(|a| a.side != rgb.side || a.channels != 1) {
        return None;
    }
    let n = (rgb.side * rgb.side) as usize;
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let p = &rgb.data[i * rgb.channels..];
        out.extend_from_slice(&[p[0], p[1], p[2], alpha.map_or(255, |a| a.data[i])]);
    }
    Some(out)
}

/// The next mip down: each texel the plain average of four.
fn halve(texels: &[u8], side: u32) -> Vec<u8> {
    let half = (side / 2) as usize;
    let side = side as usize;
    let mut out = Vec::with_capacity(half * half * 4);
    for y in 0..half {
        for x in 0..half {
            for c in 0..4 {
                let at = |dx: usize, dy: usize| {
                    u32::from(texels[((2 * y + dy) * side + 2 * x + dx) * 4 + c])
                };
                out.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
            }
        }
    }
    out
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    side: u32,
    texels: Vec<u8>,
) -> (wgpu::TextureView, u64) {
    let levels = side.ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = 0;
    let mut texels = texels;
    let mut s = side;
    for level in 0..levels {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &texels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(s * 4),
                rows_per_image: Some(s),
            },
            wgpu::Extent3d {
                width: s,
                height: s,
                depth_or_array_layers: 1,
            },
        );
        bytes += texels.len() as u64;
        if s > 1 {
            texels = halve(&texels, s);
            s /= 2;
        }
    }
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        bytes,
    )
}

pub fn load(device: &wgpu::Device, queue: &wgpu::Queue) -> RockTextures {
    let real = (|| {
        let albedo = decode(ALBEDO)?;
        let normal = decode(NORMAL)?;
        let rough = decode(ROUGH)?;
        let color = pack(&albedo, Some(&rough))?;
        let normals = pack(&normal, None)?;
        (normal.side == albedo.side).then_some((albedo.side, color, normals))
    })();
    let (side, color, normals, real) = match real {
        Some((side, color, normals)) => (side, color, normals, true),
        None => (1, vec![128, 128, 128, 255], vec![128, 128, 255, 255], false),
    };
    let (color, a) = upload(
        device,
        queue,
        "rock albedo",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        side,
        color,
    );
    let (normal, b) = upload(
        device,
        queue,
        "rock normal",
        wgpu::TextureFormat::Rgba8Unorm,
        side,
        normals,
    );
    RockTextures {
        color,
        normal,
        real,
        bytes: a + b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rock_pngs_decode_and_match() {
        let albedo = decode(ALBEDO).expect("albedo");
        let normal = decode(NORMAL).expect("normal");
        let rough = decode(ROUGH).expect("roughness");
        assert_eq!(albedo.side, normal.side);
        assert_eq!(albedo.side, rough.side);
        assert_eq!(
            pack(&albedo, Some(&rough)).unwrap().len(),
            (albedo.side * albedo.side * 4) as usize
        );
    }

    #[test]
    fn halving_averages() {
        let two = [10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255];
        assert_eq!(halve(&two, 2), vec![25, 0, 0, 255]);
    }
}
