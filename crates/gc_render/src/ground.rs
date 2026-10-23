//! The ground textures `fs_terrain` blends under the painted palette: ambientCG
//! Grass001 (layer 0) and Ground054 (layer 1), CC0, baked to 512 px by
//! `tools/ground_textures/bake.py` and compiled in like the rock's. They
//! load as two texture arrays, albedo and normal, with CPU-built mips
//! (`rock::upload`): about 5.3 MB of GPU memory with mips. A test decodes all
//! four, so `load` may expect them. They come back as `RockTextures`, which
//! here hold arrays: grass then soil, albedo (sRGB) and OpenGL normals.

use crate::rock::{self, Mip, RockTextures};

const GRASS_ALBEDO: &[u8] = include_bytes!("../../../assets/textures/terrain/grass001_albedo.png");
const GRASS_NORMAL: &[u8] = include_bytes!("../../../assets/textures/terrain/grass001_normal.png");
const SOIL_ALBEDO: &[u8] = include_bytes!("../../../assets/textures/terrain/ground054_albedo.png");
const SOIL_NORMAL: &[u8] = include_bytes!("../../../assets/textures/terrain/ground054_normal.png");

/// Texels of each image, in layer order: `rgb` is a pair (grass, soil) of PNGs.
fn layers(rgb: [(&'static str, &[u8]); 2]) -> (u32, Vec<Vec<u8>>) {
    let mut side = 0;
    let layers = rgb
        .map(|(name, png)| {
            let image = rock::decode(png).unwrap_or_else(|| panic!("{name} decodes"));
            assert!(
                side == 0 || side == image.side,
                "{name} matches the other layer's size"
            );
            side = image.side;
            rock::pack(&image, None).unwrap_or_else(|| panic!("{name} is RGB"))
        })
        .into();
    (side, layers)
}

pub fn load(device: &wgpu::Device, queue: &wgpu::Queue) -> RockTextures {
    let (side, colors) = layers([
        ("grass001_albedo", GRASS_ALBEDO),
        ("ground054_albedo", SOIL_ALBEDO),
    ]);
    let (normal_side, normals) = layers([
        ("grass001_normal", GRASS_NORMAL),
        ("ground054_normal", SOIL_NORMAL),
    ]);
    assert_eq!(
        side, normal_side,
        "ground albedo and normal maps are the same size"
    );
    let (color, a) = rock::upload(
        device,
        queue,
        "ground albedo",
        wgpu::TextureFormat::Rgba8UnormSrgb,
        side,
        colors,
        Mip::Color,
    );
    let (normal, b) = rock::upload(
        device,
        queue,
        "ground normal",
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
    fn the_ground_pngs_decode_and_match() {
        let (side, colors) = layers([("grass", GRASS_ALBEDO), ("soil", SOIL_ALBEDO)]);
        let (normal_side, normals) = layers([("grass", GRASS_NORMAL), ("soil", SOIL_NORMAL)]);
        assert_eq!(side, normal_side);
        assert!(
            colors
                .iter()
                .chain(&normals)
                .all(|l| l.len() == (side * side * 4) as usize)
        );
    }

    /// The mean colours in world.wgsl (GRASS_MEAN, SOIL_MEAN) are those of
    /// the albedo maps, linear.
    #[test]
    fn the_shader_knows_the_albedo_means() {
        let shader = include_str!("shaders/world.wgsl");
        for (name, png) in [("GRASS_MEAN", GRASS_ALBEDO), ("SOIL_MEAN", SOIL_ALBEDO)] {
            let line = shader
                .lines()
                .find(|l| l.contains(&format!("const {name}")))
                .unwrap_or_else(|| panic!("{name} is in world.wgsl"));
            let written: Vec<f32> = line
                [line.find("vec3<f32>(").unwrap() + 10..line.rfind(')').unwrap()]
                .split(',')
                .map(|v| v.trim().parse().unwrap())
                .collect();
            let image = rock::decode(png).unwrap();
            let texels = (image.side * image.side) as usize;
            let texels_rgba = rock::pack(&image, None).unwrap();
            for c in 0..3 {
                let mean = texels_rgba
                    .chunks_exact(4)
                    .map(|t| rock::to_linear(t[c]))
                    .sum::<f32>()
                    / texels as f32;
                assert!(
                    (mean - written[c]).abs() < 0.02 * mean.max(0.05),
                    "{name}[{c}]: shader has {}, texture is {mean}",
                    written[c]
                );
            }
        }
    }
}
