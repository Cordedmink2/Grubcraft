//! The Rock035 textures `fs_terrain` paints mountains with. The PNGs under
//! `assets/textures/terrain` are compiled into the binary (like the shaders,
//! so no packaging step has to find them), decoded at start-up, given a
//! mip chain on the CPU and uploaded as two textures: albedo with roughness
//! in alpha, and the normal map. Together they are about 2.8 MB of GPU
//! memory with mips. A test decodes all three, so `load` may expect them.

const ALBEDO: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_albedo.png");
const NORMAL: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_normal.png");
const ROUGH: &[u8] = include_bytes!("../../../assets/textures/terrain/rock035_rough.png");

/// The rock textures.
pub struct RockTextures {
    pub color: wgpu::TextureView,
    pub normal: wgpu::TextureView,
    pub bytes: u64,
}

/// How a mip level is made from the one above.
#[derive(Clone, Copy)]
enum Mip {
    /// sRGB colour (averaged as light) with linear data in alpha.
    Color,
    /// A tangent-space normal map: averaged as vectors, then renormalised.
    Normal,
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

fn to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(l: f32) -> u8 {
    let c = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The next mip down: each texel the average of four, by `mip`'s rule.
fn halve(texels: &[u8], side: u32, mip: Mip) -> Vec<u8> {
    let half = (side / 2) as usize;
    let side = side as usize;
    let mut out = Vec::with_capacity(half * half * 4);
    for y in 0..half {
        for x in 0..half {
            let at =
                |dx: usize, dy: usize, c: usize| texels[((2 * y + dy) * side + 2 * x + dx) * 4 + c];
            let cells = [(0, 0), (1, 0), (0, 1), (1, 1)];
            let plain = |c: usize| {
                let sum: u32 = cells.iter().map(|&(dx, dy)| u32::from(at(dx, dy, c))).sum();
                ((sum + 2) / 4) as u8
            };
            match mip {
                Mip::Color => {
                    for c in 0..3 {
                        let sum: f32 = cells.iter().map(|&(dx, dy)| to_linear(at(dx, dy, c))).sum();
                        out.push(to_srgb(sum / 4.0));
                    }
                    out.push(plain(3));
                }
                Mip::Normal => {
                    let mut v = [0.0f32; 3];
                    for (c, v) in v.iter_mut().enumerate() {
                        let sum: f32 = cells
                            .iter()
                            .map(|&(dx, dy)| f32::from(at(dx, dy, c)) / 127.5 - 1.0)
                            .sum();
                        *v = sum / 4.0;
                    }
                    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-4);
                    for v in v {
                        out.push(((v / len * 0.5 + 0.5) * 255.0 + 0.5) as u8);
                    }
                    out.push(255);
                }
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
    mip: Mip,
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
            texels = halve(&texels, s, mip);
            s /= 2;
        }
    }
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        bytes,
    )
}

pub fn load(device: &wgpu::Device, queue: &wgpu::Queue) -> RockTextures {
    let albedo = decode(ALBEDO).expect("rock035_albedo.png decodes");
    let normal = decode(NORMAL).expect("rock035_normal.png decodes");
    let rough = decode(ROUGH).expect("rock035_rough.png decodes");
    let color = pack(&albedo, Some(&rough)).expect("rock035 albedo and roughness match");
    let normals = pack(&normal, None).expect("rock035 normal map is RGB");
    let side = albedo.side;
    assert_eq!(side, normal.side, "rock035 maps are the same size");
    let (color, a) = upload(
        device,
        queue,
        "rock albedo",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        side,
        color,
        Mip::Color,
    );
    let (normal, b) = upload(
        device,
        queue,
        "rock normal",
        wgpu::TextureFormat::Rgba8Unorm,
        side,
        normals,
        Mip::Normal,
    );
    RockTextures {
        color,
        normal,
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
        // Colour is averaged as light: black and white make sRGB 188, not 128.
        let bw = [
            0, 0, 0, 10, 255, 255, 255, 20, 0, 0, 0, 30, 255, 255, 255, 40,
        ];
        let c = halve(&bw, 2, Mip::Color);
        assert!((186..=190).contains(&c[0]), "{c:?}");
        assert_eq!(c[3], 25);
        // Normals are renormalised: two tilted ones average to a unit vector.
        let tilt = [
            255, 128, 255, 255, 0, 128, 255, 255, 255, 128, 255, 255, 0, 128, 255, 255,
        ];
        let n = halve(&tilt, 2, Mip::Normal);
        assert!(n[0].abs_diff(128) <= 1 && n[2] >= 254, "{n:?}");
    }
}
