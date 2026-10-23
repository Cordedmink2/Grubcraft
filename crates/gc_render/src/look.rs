//! The look of the light: where the sun stands and what colour it is, the
//! air between the camera and the hills, the sky, and the colour grade the
//! frame goes through at the end. A [`Look`] is plain numbers, so a map (or
//! a lobby's "Time of day") can pick a [`LookPreset`] and a tuning panel can
//! move any value live. Render only: nothing here touches the simulation.
//!
//! Colours that are drawn *as they appear* (fog, haze, the sky) are given in
//! sRGB, the way an artist reads them; [`Look::hdr`] turns one into the HDR
//! value that the film curve in `shaders/post.wgsl` maps back to it.
//! Lighting colours (the sun, the ambient light) are HDR already.

use glam::Vec3;

/// The prepared looks. Golden hour is the game's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LookPreset {
    /// A bright, high sun in clear air.
    Noon,
    /// A low warm sun behind the field, blue shade, long shadows.
    #[default]
    GoldenHour,
    /// Soft grey light and heavy air; few shadows.
    Overcast,
    /// Steel light, fast cloud shadows, low colour.
    Storm,
    /// The sun just above the hills: violet shade, an orange sky.
    Dusk,
}

impl LookPreset {
    pub const ALL: [LookPreset; 5] = [
        LookPreset::Noon,
        LookPreset::GoldenHour,
        LookPreset::Overcast,
        LookPreset::Storm,
        LookPreset::Dusk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LookPreset::Noon => "Noon",
            LookPreset::GoldenHour => "Golden hour",
            LookPreset::Overcast => "Overcast",
            LookPreset::Storm => "Storm",
            LookPreset::Dusk => "Dusk",
        }
    }

    /// The preset a name (as typed on a command line) stands for, ignoring
    /// case, spaces, dashes and underscores.
    pub fn from_name(name: &str) -> Option<LookPreset> {
        let key = |s: &str| {
            s.chars()
                .filter(char::is_ascii_alphanumeric)
                .map(|c| c.to_ascii_lowercase())
                .collect::<String>()
        };
        let want = key(name);
        LookPreset::ALL.into_iter().find(|p| key(p.name()) == want)
    }

    /// The next preset in [`LookPreset::ALL`], wrapping round.
    pub fn next(self) -> LookPreset {
        let at = LookPreset::ALL.iter().position(|&p| p == self).unwrap_or(0);
        LookPreset::ALL[(at + 1) % LookPreset::ALL.len()]
    }

    pub fn look(self) -> Look {
        match self {
            LookPreset::GoldenHour => GOLDEN_HOUR,
            LookPreset::Noon => Look {
                sun: [-0.45, 0.75, -0.35],
                sun_color: [1.9, 1.71, 1.41],
                sky_ambient: [0.16, 0.20, 0.28],
                ground_ambient: [0.11, 0.08, 0.055],
                fog: [0.62, 0.68, 0.76],
                haze: [0.74, 0.76, 0.78],
                fog_density: 0.0014,
                fog_falloff: 0.04,
                rim: 0.35,
                foliage_glow: 0.3,
                cloud_shadows: 0.2,
                ground_occlusion: 0.8,
                contrast: 1.06,
                saturation: 1.06,
                vignette: 0.18,
                grain: 0.02,
                light_tint: [1.02, 1.0, 0.98],
                sky_top: [0.30, 0.46, 0.72],
                sky_horizon: [0.74, 0.82, 0.90],
                ..GOLDEN_HOUR
            },
            LookPreset::Overcast => Look {
                sun: [-0.25, 0.80, -0.30],
                sun_color: [1.15, 1.15, 1.18],
                sky_ambient: [0.26, 0.29, 0.34],
                ground_ambient: [0.13, 0.12, 0.11],
                fog: [0.60, 0.63, 0.66],
                haze: [0.66, 0.68, 0.70],
                fog_density: 0.0022,
                fog_falloff: 0.04,
                rim: 0.3,
                foliage_glow: 0.1,
                cloud_shadows: 0.5,
                exposure: 0.945,
                contrast: 1.12,
                saturation: 0.9,
                vignette: 0.25,
                shadow_tint: [0.98, 1.0, 1.03],
                light_tint: [1.0, 1.0, 1.0],
                sky_top: [0.46, 0.50, 0.55],
                sky_horizon: [0.68, 0.70, 0.72],
                ..GOLDEN_HOUR
            },
            LookPreset::Storm => Look {
                sun: [-0.35, 0.62, -0.55],
                sun_color: [1.7, 1.72, 1.78],
                sky_ambient: [0.15, 0.17, 0.20],
                ground_ambient: [0.08, 0.075, 0.07],
                fog: [0.50, 0.54, 0.58],
                haze: [0.66, 0.68, 0.70],
                fog_density: 0.0024,
                fog_falloff: 0.04,
                rim: 0.6,
                foliage_glow: 0.15,
                cloud_shadows: 0.8,
                contrast: 1.3,
                saturation: 0.8,
                vignette: 0.42,
                grain: 0.04,
                light_tint: [1.02, 1.01, 0.99],
                sky_top: [0.28, 0.31, 0.36],
                sky_horizon: [0.55, 0.58, 0.61],
                ..GOLDEN_HOUR
            },
            LookPreset::Dusk => Look {
                sun: [-0.80, 0.17, -0.58],
                sun_color: [2.2, 1.6, 1.2],
                sky_ambient: [0.13, 0.17, 0.32],
                ground_ambient: [0.07, 0.07, 0.09],
                fog: [0.42, 0.46, 0.62],
                haze: [0.90, 0.62, 0.48],
                fog_density: 0.0020,
                fog_falloff: 0.06,
                rim: 1.2,
                foliage_glow: 0.7,
                cloud_shadows: 0.25,
                exposure: 1.035,
                contrast: 1.12,
                saturation: 0.95,
                vignette: 0.40,
                grain: 0.035,
                shadow_tint: [0.95, 0.98, 1.10],
                light_tint: [1.03, 0.99, 0.97],
                sky_top: [0.16, 0.20, 0.40],
                sky_horizon: [0.95, 0.62, 0.46],
                ..GOLDEN_HOUR
            },
        }
    }
}

/// The game's default look, and what the other presets change from.
const GOLDEN_HOUR: Look = Look {
    sun: [-0.62, 0.36, -0.70],
    sun_color: [2.6, 2.05, 1.45],
    sky_ambient: [0.17, 0.22, 0.33],
    ground_ambient: [0.12, 0.09, 0.06],
    fog: [0.70, 0.74, 0.82],
    haze: [0.92, 0.80, 0.62],
    fog_density: 0.0016,
    fog_falloff: 0.05,
    rim: 0.9,
    foliage_glow: 0.45,
    cloud_shadows: 0.4,
    ground_occlusion: 1.0,
    exposure: 0.9,
    contrast: 1.1,
    saturation: 1.15,
    vignette: 0.30,
    grain: 0.03,
    shadow_tint: [0.97, 1.0, 1.04],
    light_tint: [1.03, 1.0, 0.97],
    sky_top: [0.32, 0.48, 0.74],
    sky_horizon: [0.93, 0.83, 0.70],
};

/// Everything about how the world is lit and how the frame is graded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Direction toward the sun (x east, y up, z south); normalised on use.
    pub sun: [f32; 3],
    /// The sun's light on a surface facing it, HDR (about 1.7 is sunlit
    /// white paint).
    pub sun_color: [f32; 3],
    /// Light from the sky onto surfaces facing up, and from the ground onto
    /// those facing down, HDR; the two blend by how much a surface faces up.
    pub sky_ambient: [f32; 3],
    pub ground_ambient: [f32; 3],
    /// Air colour (sRGB) looking away from the sun, and toward it.
    pub fog: [f32; 3],
    pub haze: [f32; 3],
    /// How thick the air is at sea level (height 0), per tile of view ray.
    pub fog_density: f32,
    /// How quickly the air thins with height, per tile.
    pub fog_falloff: f32,
    /// Back-light rim on soldiers and buildings, 0 off.
    pub rim: f32,
    /// How strongly leaves and grass glow when the sun is behind them.
    pub foliage_glow: f32,
    /// How dark the clouds' shadows drifting over the land are, 0 none.
    pub cloud_shadows: f32,
    /// How strongly the ground is darkened in valleys and at the foot of
    /// slopes, 0 off.
    pub ground_occlusion: f32,
    /// The film curve's input scale.
    pub exposure: f32,
    /// Contrast round mid grey, 1 neutral.
    pub contrast: f32,
    pub saturation: f32,
    /// How dark the screen's corners get, 0 none.
    pub vignette: f32,
    /// Film grain, as a fraction of the display range; 0 off.
    pub grain: f32,
    /// Tints multiplied in on the dark and the bright ends (split toning).
    pub shadow_tint: [f32; 3],
    pub light_tint: [f32; 3],
    /// The sky overhead and at the horizon, sRGB.
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
}

impl Default for Look {
    fn default() -> Look {
        LookPreset::default().look()
    }
}

impl Look {
    /// The sun's direction, normalised.
    pub fn sun_dir(&self) -> Vec3 {
        let v = Vec3::from(self.sun);
        if v.length_squared() > 1e-6 {
            v.normalize()
        } else {
            Vec3::Y
        }
    }

    /// The HDR value that the film curve alone draws as the sRGB colour `c`
    /// (gamma 2.2, as the shaders use). The grade's tints, contrast,
    /// saturation and vignette come after the curve, so a colour lands near
    /// what was written, not exactly on it: the presets are tuned by eye.
    pub fn hdr(&self, c: [f32; 3]) -> [f32; 3] {
        c.map(|v| inverse_film(v.max(0.0).powf(2.2)) / self.exposure.max(0.05))
    }
}

/// A colour and one more number, packed for a shader's `vec4`.
pub(crate) fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// The film curve (Narkowicz's fit of the ACES curve, as in `post.wgsl`).
#[cfg(test)]
pub(crate) fn film(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// The input that [`film`] maps to `y` (0 to just under 1): the positive
/// root of `(2.51 - 2.43 y) x² + (0.03 - 0.59 y) x - 0.14 y = 0`.
pub(crate) fn inverse_film(y: f32) -> f32 {
    let y = y.clamp(0.0, 0.995);
    let a = 2.51 - 2.43 * y;
    let b = 0.03 - 0.59 * y;
    let c = -0.14 * y;
    (-b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
}

/// Ground occlusion, one byte per tile of a map `size` samples across
/// (`samples_per_tile` per tile edge, heights as the renderer's heightmap
/// has them: `max_height` tiles at `u16::MAX`): 255 on open ground, down to
/// about 100 deep in a valley or at the foot of a steep slope. Computed
/// once per map, from the heights alone.
pub fn ground_occlusion(
    heights: &[u16],
    size: u32,
    samples_per_tile: u32,
    max_height: f32,
) -> (u32, Vec<u8>) {
    let tiles = (size.saturating_sub(1) / samples_per_tile.max(1)) as usize;
    if tiles == 0 {
        return (1, vec![255]);
    }
    let size = size as usize;
    let per = samples_per_tile as f32;
    let scale = max_height / f32::from(u16::MAX);
    // Bilinear height at a position in tiles, clamped to the map.
    let height = |x: f32, z: f32| {
        let (sx, sz) = (x * per, z * per);
        let max = (size - 1) as f32;
        let (sx, sz) = (sx.clamp(0.0, max), sz.clamp(0.0, max));
        let (ix, iz) = (sx.floor() as usize, sz.floor() as usize);
        let (fx, fz) = (sx - ix as f32, sz - iz as f32);
        let at = |x: usize, z: usize| f32::from(heights[z.min(size - 1) * size + x.min(size - 1)]);
        let top = at(ix, iz) + (at(ix + 1, iz) - at(ix, iz)) * fx;
        let bottom = at(ix, iz + 1) + (at(ix + 1, iz + 1) - at(ix, iz + 1)) * fx;
        (top + (bottom - top) * fz) * scale
    };
    let row = |tz: usize, out: &mut [u8]| {
        for (tx, byte) in out.iter_mut().enumerate() {
            let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
            let h0 = height(x, z);
            let mut occ = 0.0;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                for r in [2.5f32, 10.0] {
                    // How steeply the horizon rises in this direction.
                    occ += ((height(x + c * r, z + s * r) - h0) / r).clamp(0.0, 1.0);
                }
            }
            let dark = (occ / 16.0 * 2.2).clamp(0.0, 0.6);
            *byte = ((1.0 - dark) * 255.0).round() as u8;
        }
    };
    let mut out = vec![255u8; tiles * tiles];
    // Rows are independent: spread bands of them over the cores, leaving
    // two for the render and simulation threads.
    let band = tiles
        .div_ceil(gc_platform::thread::workers_leaving(2))
        .max(1);
    std::thread::scope(|scope| {
        for (k, rows) in out.chunks_mut(band * tiles).enumerate() {
            let row = &row;
            gc_platform::thread::spawn_scoped_tagged(
                scope,
                gc_platform::tagged_alloc::Tag::Render,
                move || {
                    for (j, line) in rows.chunks_mut(tiles).enumerate() {
                        row(k * band + j, line);
                    }
                },
            );
        }
    });
    (tiles as u32, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_film_curve_inverts() {
        for y in [0.05, 0.2, 0.5, 0.8, 0.95] {
            assert!((film(inverse_film(y)) - y).abs() < 1e-3, "{y}");
        }
    }

    #[test]
    fn hdr_colours_draw_back_as_themselves() {
        let look = LookPreset::GoldenHour.look();
        for c in [[0.7, 0.74, 0.82], [0.93, 0.83, 0.7]] {
            let h = look.hdr(c);
            let back = h.map(|v| film(v * look.exposure).powf(1.0 / 2.2));
            for k in 0..3 {
                assert!((back[k] - c[k]).abs() < 0.01, "{c:?} → {back:?}");
            }
        }
    }

    #[test]
    fn presets_are_found_by_name_and_cycle() {
        assert_eq!(
            LookPreset::from_name("golden-hour"),
            Some(LookPreset::GoldenHour)
        );
        assert_eq!(LookPreset::from_name("STORM"), Some(LookPreset::Storm));
        assert_eq!(LookPreset::from_name("night"), None);
        let mut p = LookPreset::Noon;
        for _ in 0..LookPreset::ALL.len() {
            p = p.next();
        }
        assert_eq!(p, LookPreset::Noon);
    }

    #[test]
    fn every_preset_has_a_sun_above_the_horizon() {
        for p in LookPreset::ALL {
            assert!(p.look().sun_dir().y > 0.05, "{}", p.name());
        }
    }

    #[test]
    fn flat_ground_is_open_and_a_pit_is_dark() {
        let (size, per) = (33u32, 1u32);
        let mut heights = vec![1000u16; (size * size) as usize];
        let (side, flat) = ground_occlusion(&heights, size, per, 10.0);
        assert_eq!(side, 32);
        assert!(flat.iter().all(|&v| v == 255));
        // Sink a pit in the middle.
        let mid = (size / 2) as usize;
        heights[mid * size as usize + mid] = 0;
        heights[mid * size as usize + mid - 1] = 0;
        let (_, pit) = ground_occlusion(&heights, size, per, 10.0);
        assert!(pit[(mid - 1) * 32 + mid - 1] < 255);
        assert_eq!(pit[0], 255);
    }
}
