//! Material textures: the painted maps a model's UV atlas points into,
//! shrunk to [`TEXTURE_SIZE`], packed into the three layers the soldier
//! shader reads (see [`gc_assets::model::MaterialLayer`]), each with its full
//! mip chain.

use std::path::Path;

use gc_assets::model::{TEXTURE_MIPS, TEXTURE_SIZE};
use glam::Vec3;
use serde::Deserialize;

use crate::Image;

/// PNGs painted on the model's UV atlas, as `shared:` or `repo:` paths.
/// Every map must be square, [`TEXTURE_SIZE`] or a power of two larger.
#[derive(Deserialize)]
pub struct TextureConfig {
    /// Base colour (sRGB).
    pub albedo: String,
    /// White where the team colour replaces the base colour.
    #[serde(default)]
    pub team: Option<String>,
    /// Tangent-space normals, OpenGL convention (green up).
    #[serde(default)]
    pub normal: Option<String>,
    /// Red: ambient occlusion, green: roughness, blue: metalness.
    #[serde(default)]
    pub orm: Option<String>,
    /// Emitted light (sRGB).
    #[serde(default)]
    pub emissive: Option<String>,
}

/// Roughness of a material without an `orm` map.
const DEFAULT_ROUGHNESS: f32 = 0.7;

type Px = [f32; 4];

/// How a map's channels are stored and filtered.
#[derive(Clone, Copy, PartialEq)]
enum Encoding {
    /// Red, green and blue in sRGB; alpha linear.
    Srgb,
    Linear,
    /// Red and green are a tangent-space normal's x and y, averaged as a
    /// vector; blue and alpha linear.
    Normal,
}

/// A square image in linear values (sRGB channels already decoded).
struct Layer {
    side: usize,
    px: Vec<Px>,
}

impl Layer {
    fn filled(px: Px) -> Layer {
        let side = TEXTURE_SIZE as usize;
        Layer {
            side,
            px: vec![px; side * side],
        }
    }

    /// Half the size, averaging 2 × 2 blocks (values are linear already).
    fn half(&self, enc: Encoding) -> Layer {
        let side = (self.side / 2).max(1);
        let mut px = Vec::with_capacity(side * side);
        for y in 0..side {
            for x in 0..side {
                let mut sum = [0.0; 4];
                let mut n = Vec3::ZERO;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(self.side - 1);
                    let sy = (y * 2 + dy).min(self.side - 1);
                    let p = self.px[sy * self.side + sx];
                    for c in 0..4 {
                        sum[c] += p[c] * 0.25;
                    }
                    if enc == Encoding::Normal {
                        n += decode_normal(p);
                    }
                }
                if enc == Encoding::Normal {
                    let n = n.normalize_or(Vec3::Z);
                    sum[0] = n.x * 0.5 + 0.5;
                    sum[1] = n.y * 0.5 + 0.5;
                }
                px.push(sum);
            }
        }
        Layer { side, px }
    }
}

fn decode_normal(p: Px) -> Vec3 {
    let x = p[0] * 2.0 - 1.0;
    let y = p[1] * 2.0 - 1.0;
    Vec3::new(x, y, (1.0 - x * x - y * y).max(0.0).sqrt())
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Read a PNG.
pub fn load_png(path: &Path) -> Result<Image, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::decode_png(&data).map_err(|e| format!("{}: {e}", path.display()))
}

/// Shrink a map to [`TEXTURE_SIZE`], decoding sRGB channels to linear.
fn shrink(img: &Image, path: &Path, enc: Encoding) -> Result<Layer, String> {
    let side = img.width as usize;
    let target = TEXTURE_SIZE as usize;
    if img.width != img.height || side < target || !side.is_power_of_two() {
        return Err(format!(
            "{}: {} × {} texels; maps must be square, {target} or a power of two larger",
            path.display(),
            img.width,
            img.height
        ));
    }
    let px = img
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| {
            let f = |i: usize| c[i] as f32 / 255.0;
            let col = |i: usize| {
                if enc == Encoding::Srgb {
                    srgb_to_linear(f(i))
                } else {
                    f(i)
                }
            };
            [col(0), col(1), col(2), f(3)]
        })
        .collect();
    let mut layer = Layer { side, px };
    while layer.side > target {
        layer = layer.half(enc);
    }
    Ok(layer)
}

/// The three layers of one material, each with its mips, back to back.
/// `albedo` is the `cfg.albedo` map, already loaded from `albedo_path`.
pub fn bake(
    cfg: &TextureConfig,
    albedo: &Image,
    albedo_path: &Path,
    resolve: impl Fn(&str) -> Result<std::path::PathBuf, String>,
) -> Result<Vec<[u8; 4]>, String> {
    let map = |src: &Option<String>, enc: Encoding| -> Result<Option<Layer>, String> {
        src.as_ref()
            .map(|s| {
                let path = resolve(s)?;
                shrink(&load_png(&path)?, &path, enc)
            })
            .transpose()
    };
    let albedo = shrink(albedo, albedo_path, Encoding::Srgb)?;
    let team = map(&cfg.team, Encoding::Linear)?;
    let normal = map(&cfg.normal, Encoding::Normal)?.unwrap_or(Layer::filled([0.5, 0.5, 1.0, 1.0]));
    let orm = map(&cfg.orm, Encoding::Linear)?.unwrap_or(Layer::filled([
        1.0,
        DEFAULT_ROUGHNESS,
        0.0,
        1.0,
    ]));
    let emissive = map(&cfg.emissive, Encoding::Srgb)?.unwrap_or(Layer::filled([0.0; 4]));

    let n = albedo.px.len();
    let layer = |f: &dyn Fn(usize) -> Px| Layer {
        side: albedo.side,
        px: (0..n).map(f).collect(),
    };
    let a = &albedo.px;
    let layers = [
        // Albedo and team mask.
        (
            layer(&|i| {
                let t = team.as_ref().map_or(0.0, |t| t.px[i][0]);
                [a[i][0], a[i][1], a[i][2], t]
            }),
            Encoding::Srgb,
        ),
        // Normal, occlusion, roughness.
        (
            layer(&|i| {
                let (nm, o) = (normal.px[i], orm.px[i]);
                [nm[0], nm[1], o[0], o[1]]
            }),
            Encoding::Normal,
        ),
        // Emitted light and metalness.
        (
            layer(&|i| {
                let (e, o) = (emissive.px[i], orm.px[i]);
                [e[0], e[1], e[2], o[2]]
            }),
            Encoding::Srgb,
        ),
    ];
    let mut out = Vec::new();
    for (mut level, enc) in layers {
        for mip in 0..TEXTURE_MIPS {
            if mip > 0 {
                level = level.half(enc);
            }
            out.extend(level.px.iter().map(|p| {
                let byte = |c: usize| {
                    let v = if enc == Encoding::Srgb && c < 3 {
                        linear_to_srgb(p[c])
                    } else {
                        p[c]
                    };
                    (v.clamp(0.0, 1.0) * 255.0).round() as u8
                };
                [byte(0), byte(1), byte(2), byte(3)]
            }));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_assets::model::{TEXELS_PER_LAYER, TEXELS_PER_MATERIAL};

    #[test]
    fn halving_keeps_normals_unit_and_averages_colour() {
        let side = 4;
        let tilt = 0.5 + 0.5 * 0.6;
        let layer = Layer {
            side,
            px: (0..side * side)
                .map(|i| {
                    if i % 2 == 0 {
                        [tilt, 0.5, 1.0, 0.0]
                    } else {
                        [0.5, 0.5, 1.0, 1.0]
                    }
                })
                .collect(),
        };
        let h = layer.half(Encoding::Normal);
        assert_eq!(h.side, 2);
        let n = decode_normal(h.px[0]);
        assert!((n.length() - 1.0).abs() < 1e-4);
        assert!(n.x > 0.0 && n.x < 0.6);
        assert!((h.px[0][3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn srgb_round_trips() {
        for v in [0.0, 0.002, 0.2, 0.5, 1.0] {
            assert!((linear_to_srgb(srgb_to_linear(v)) - v).abs() < 1e-5);
        }
    }

    #[test]
    fn a_material_is_three_layers_with_mips() {
        let dir = std::env::temp_dir().join(format!("gc_artbake_tex_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("albedo.png");
        let side = TEXTURE_SIZE * 2;
        let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), side, side);
        enc.set_color(png::ColorType::Rgb);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&vec![200u8; (side * side * 3) as usize])
            .unwrap();
        w.finish().unwrap();
        let cfg = TextureConfig {
            albedo: "albedo.png".into(),
            team: None,
            normal: None,
            orm: None,
            emissive: None,
        };
        let albedo = load_png(&path).unwrap();
        let texels = bake(&cfg, &albedo, &path, |s| Ok(dir.join(s))).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(texels.len(), TEXELS_PER_MATERIAL);
        // Albedo survives shrinking; no team; a flat normal; default roughness.
        assert_eq!(texels[0], [200, 200, 200, 0]);
        let b = texels[TEXELS_PER_LAYER];
        assert_eq!((b[0], b[1], b[2]), (128, 128, 255));
        assert_eq!(b[3], (DEFAULT_ROUGHNESS * 255.0).round() as u8);
        assert_eq!(texels[2 * TEXELS_PER_LAYER], [0, 0, 0, 0]);
    }
}
