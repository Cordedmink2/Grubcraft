//! The detail textures `fs_terrain` and `fs_scenery` blend under the painted
//! colour, all ambientCG, CC0, baked to 512 px by
//! `tools/ground_textures/bake.py` and compiled in like the rock's. Each set
//! loads as two texture arrays, albedo and normal, with CPU-built mips
//! (`rock::upload`), about 2.7 MB of GPU memory a layer pair with mips, and
//! comes back as `RockTextures`, which here hold arrays. A test decodes every
//! PNG, so the loaders may expect them.
//!
//! - The ground: Grass001 (layer 0) and Ground054 (layer 1).
//! - Scenery: Bark001 (0, broadleaf bark), Bark014 (1, conifer bark) and
//!   Moss002 (2, moss and foliage). Stone uses the rock's own textures.

use crate::rock::{self, Mip, RockTextures};

macro_rules! png {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/textures/terrain/", $name, ".png")) as &[u8]
    };
}

/// A texture set: the albedo and normal PNGs of each layer, by name.
type Set<'a> = &'a [(&'static str, &'a [u8], &'a [u8])];

pub(crate) const GROUND: Set<'static> = &[
    ("grass001", png!("grass001_albedo"), png!("grass001_normal")),
    (
        "ground054",
        png!("ground054_albedo"),
        png!("ground054_normal"),
    ),
];
pub(crate) const SCENERY: Set<'static> = &[
    ("bark001", png!("bark001_albedo"), png!("bark001_normal")),
    ("bark014", png!("bark014_albedo"), png!("bark014_normal")),
    ("moss002", png!("moss002_albedo"), png!("moss002_normal")),
];

/// Texels of each image of a set (albedo, normal), in layer order, and the
/// side they share.
fn layers(set: Set<'_>) -> (u32, Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let mut side = 0;
    let mut one = |name: &str, png: &[u8]| {
        let image = rock::decode(png).unwrap_or_else(|| panic!("{name} decodes"));
        assert!(
            side == 0 || side == image.side,
            "{name} is the size of the others"
        );
        side = image.side;
        rock::pack(&image, None).unwrap_or_else(|| panic!("{name} is RGB"))
    };
    let mut colors = Vec::new();
    let mut normals = Vec::new();
    for &(name, color, normal) in set {
        colors.push(one(&format!("{name}_albedo"), color));
        normals.push(one(&format!("{name}_normal"), normal));
    }
    (side, colors, normals)
}

/// `set` as an array of albedo (sRGB) and one of OpenGL tangent-space normals.
pub fn load(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, set: Set<'_>) -> RockTextures {
    let (side, colors, normals) = layers(set);
    let (color, a) = rock::upload(
        device,
        queue,
        &format!("{label} albedo"),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        side,
        colors,
        Mip::Color,
    );
    let (normal, b) = rock::upload(
        device,
        queue,
        &format!("{label} normal"),
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
    fn the_pngs_decode_and_match() {
        for set in [GROUND, SCENERY] {
            let (side, colors, normals) = layers(set);
            assert_eq!(colors.len(), set.len());
            assert!(
                colors
                    .iter()
                    .chain(&normals)
                    .all(|l| l.len() == (side * side * 4) as usize)
            );
        }
    }

    /// The mean colours in the shaders (`*_MEAN` in world.wgsl and
    /// scenery.wgsl) are those of the albedo maps, linear.
    #[test]
    fn the_shaders_know_the_albedo_means() {
        let shaders = [
            include_str!("shaders/world.wgsl"),
            include_str!("shaders/scenery.wgsl"),
        ];
        for (name, png) in [
            ("GRASS_MEAN", GROUND[0].1),
            ("SOIL_MEAN", GROUND[1].1),
            ("BARK_BROAD_MEAN", SCENERY[0].1),
            ("BARK_CONIFER_MEAN", SCENERY[1].1),
            ("MOSS_MEAN", SCENERY[2].1),
        ] {
            let line = shaders
                .iter()
                .flat_map(|s| s.lines())
                .find(|l| l.contains(&format!("const {name}")))
                .unwrap_or_else(|| panic!("{name} is in a shader"));
            let written: Vec<f32> = line
                [line.find("vec3<f32>(").unwrap() + 10..line.rfind(')').unwrap()]
                .split(',')
                .map(|v| v.trim().parse().unwrap())
                .collect();
            let image = rock::decode(png).unwrap();
            let texels = (image.side * image.side) as usize;
            let rgba = rock::pack(&image, None).unwrap();
            for c in 0..3 {
                let mean = rgba
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
