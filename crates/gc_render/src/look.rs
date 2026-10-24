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
    /// Moonlight: deep blue shade and a dark sky, the moon low enough that
    /// the game's own "it is dark" test (the sun below 0.3 high) lights
    /// the lamps and sends out fireflies. Lit places stand out; the rest is
    /// dim but readable.
    Night,
}

impl LookPreset {
    pub const ALL: [LookPreset; 6] = [
        LookPreset::Noon,
        LookPreset::GoldenHour,
        LookPreset::Overcast,
        LookPreset::Storm,
        LookPreset::Dusk,
        LookPreset::Night,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LookPreset::Noon => "Noon",
            LookPreset::GoldenHour => "Golden hour",
            LookPreset::Overcast => "Overcast",
            LookPreset::Storm => "Storm",
            LookPreset::Dusk => "Dusk",
            LookPreset::Night => "Night",
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
            LookPreset::Night => Look {
                sun: [-0.80, 0.18, -0.50],
                sun_color: [0.34, 0.44, 0.74],
                sky_ambient: [0.05, 0.065, 0.14],
                ground_ambient: [0.022, 0.026, 0.05],
                fog: [0.05, 0.07, 0.13],
                haze: [0.10, 0.14, 0.24],
                fog_density: 0.0018,
                fog_falloff: 0.05,
                rim: 0.5,
                foliage_glow: 0.05,
                cloud_shadows: 0.2,
                exposure: 1.2,
                contrast: 1.12,
                saturation: 0.75,
                vignette: 0.45,
                grain: 0.05,
                shadow_tint: [0.9, 0.97, 1.16],
                light_tint: [0.95, 1.0, 1.1],
                sky_top: [0.015, 0.03, 0.08],
                sky_horizon: [0.07, 0.11, 0.2],
                ..GOLDEN_HOUR
            },
        }
    }
}

/// What falls from the sky, and how the air looks while it does. Render
/// only: it changes the light and the particles, never the simulation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weather {
    #[default]
    Clear,
    /// Grey light, thick air, drops streaking down.
    Rain,
    /// Pale light, soft thick air, flakes drifting down.
    Snow,
    /// No fall; the air lies thick on low ground and hides the far field.
    Fog,
}

impl Weather {
    pub const ALL: [Weather; 4] = [Weather::Clear, Weather::Rain, Weather::Snow, Weather::Fog];

    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => "Clear",
            Weather::Rain => "Rain",
            Weather::Snow => "Snow",
            Weather::Fog => "Fog",
        }
    }

    /// The weather a name (as typed on a command line) stands for, ignoring
    /// case.
    pub fn from_name(name: &str) -> Option<Weather> {
        Weather::ALL
            .into_iter()
            .find(|w| w.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The next weather in [`Weather::ALL`], wrapping round.
    pub fn next(self) -> Weather {
        let at = Weather::ALL.iter().position(|&w| w == self).unwrap_or(0);
        Weather::ALL[(at + 1) % Weather::ALL.len()]
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

impl Look {
    /// This look in a land's air: its colours shifted and its numbers
    /// scaled by `air`, whatever the light (golden hour, noon, dusk) the
    /// look began as.
    pub fn in_air(&self, air: &LandAir) -> Look {
        let shift = |c: [f32; 3], d: [f32; 3]| {
            (Vec3::from(c) + Vec3::from(d))
                .clamp(Vec3::ZERO, Vec3::ONE)
                .into()
        };
        let scale = |c: [f32; 3], m: [f32; 3]| (Vec3::from(c) * Vec3::from(m)).into();
        Look {
            sun_color: scale(self.sun_color, air.sun),
            fog: shift(self.fog, air.fog),
            haze: shift(self.haze, air.haze),
            fog_density: self.fog_density * air.density,
            fog_falloff: self.fog_falloff * air.falloff,
            contrast: self.contrast * air.contrast,
            saturation: self.saturation * air.saturation,
            shadow_tint: scale(self.shadow_tint, air.shadow_tint),
            light_tint: scale(self.light_tint, air.light_tint),
            sky_top: shift(self.sky_top, air.sky_top),
            sky_horizon: shift(self.sky_horizon, air.sky_horizon),
            ..*self
        }
    }

    /// This look under `weather` at `amount` (0 clear, 1 full): the sun
    /// dims behind cloud, the air thickens and greys, colours drain and the
    /// sky closes. It works on any look, so rain at dusk is dusk's rain.
    pub fn in_weather(&self, weather: Weather, amount: f32) -> Look {
        let amount = amount.clamp(0.0, 1.0);
        if weather == Weather::Clear || amount <= 0.0 {
            return *self;
        }
        // How the weather changes the look at full strength: the light's
        // brightness, the air's thickness and how it hugs the ground, the
        // colour left in the picture, and the grey the air and sky turn to
        // (as a share of the look's own luminance).
        let (sun, ambient, density, falloff, saturation, grey, shadows) = match weather {
            Weather::Rain => (0.35, 0.9, 2.2, 1.0, 0.7, 0.55, 2.0),
            Weather::Snow => (0.45, 1.15, 2.6, 1.0, 0.6, 0.7, 1.6),
            Weather::Fog => (0.55, 0.95, 6.0, 1.6, 0.8, 0.5, 1.0),
            Weather::Clear => unreachable!(),
        };
        let mix = |a: f32, b: f32| a + (b - a) * amount;
        let scale = |c: [f32; 3], k: f32| c.map(|v| v * mix(1.0, k));
        // A colour toward a grey of its own brightness (sRGB colours).
        let drain = |c: [f32; 3], lift: f32| {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let g = (l * mix(1.0, lift)).clamp(0.0, 1.0);
            c.map(|v| v + (g - v) * amount * grey)
        };
        let lift = if weather == Weather::Snow { 1.15 } else { 0.9 };
        Look {
            sun_color: scale(self.sun_color, sun),
            sky_ambient: scale(self.sky_ambient, ambient),
            fog: drain(self.fog, lift),
            haze: drain(self.haze, lift),
            fog_density: mix(self.fog_density, self.fog_density * density),
            fog_falloff: mix(self.fog_falloff, self.fog_falloff * falloff),
            rim: mix(self.rim, self.rim * 0.4),
            foliage_glow: mix(self.foliage_glow, self.foliage_glow * 0.4),
            cloud_shadows: mix(self.cloud_shadows, (self.cloud_shadows * shadows).min(0.9)),
            saturation: mix(self.saturation, self.saturation * saturation),
            sky_top: drain(self.sky_top, lift),
            sky_horizon: drain(self.sky_horizon, lift),
            ..*self
        }
    }

    /// The same light for competitive play (the Clarity setting): no
    /// vignette, no film grain and no heavy grading, so colours, team
    /// banners and fog of war read the same in every look. The light itself
    /// (sun, shade, air) is left as it is.
    pub fn clear(&self) -> Look {
        Look {
            contrast: self.contrast.min(CLEAR_CONTRAST),
            saturation: self.saturation.clamp(1.0, CLEAR_SATURATION),
            vignette: 0.0,
            grain: 0.0,
            shadow_tint: [1.0; 3],
            light_tint: [1.0; 3],
            ..*self
        }
    }

    /// This look with each tuning knob moved by its entry in `deltas`
    /// ([`KNOBS`] order).
    pub fn tuned(&self, deltas: &[f32; KNOBS.len()]) -> Look {
        let mut look = *self;
        for (knob, &d) in KNOBS.iter().zip(deltas) {
            if d != 0.0 {
                (knob.set)(&mut look, (knob.get)(self) + d);
            }
        }
        look
    }
}

/// The most contrast and saturation Clarity leaves in.
const CLEAR_CONTRAST: f32 = 1.05;
const CLEAR_SATURATION: f32 = 1.1;

/// What a land does to the air and the light, as changes to whatever look
/// is in use, so every land follows every time of day: colours (sRGB) are
/// added to, numbers multiplied. Neutral is a meadow's. The game blends the
/// airs of the lands round the camera ([`LandAir::mixed`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandAir {
    /// Added to the air's colour away from, and toward, the sun, and to the
    /// sky at the horizon and overhead.
    pub fog: [f32; 3],
    pub haze: [f32; 3],
    pub sky_horizon: [f32; 3],
    pub sky_top: [f32; 3],
    /// Multiply how thick the air is and how tightly it hugs low ground.
    pub density: f32,
    pub falloff: f32,
    /// Multiply the grade's saturation and contrast.
    pub saturation: f32,
    pub contrast: f32,
    /// Multiply the sun's colour and the split tints.
    pub sun: [f32; 3],
    pub shadow_tint: [f32; 3],
    pub light_tint: [f32; 3],
}

impl LandAir {
    /// Changes nothing.
    pub const NEUTRAL: LandAir = LandAir {
        fog: [0.0; 3],
        haze: [0.0; 3],
        sky_horizon: [0.0; 3],
        sky_top: [0.0; 3],
        density: 1.0,
        falloff: 1.0,
        saturation: 1.0,
        contrast: 1.0,
        sun: [1.0; 3],
        shadow_tint: [1.0; 3],
        light_tint: [1.0; 3],
    };

    /// This air toward `b`, `t` of the way.
    fn lerp(&self, b: &LandAir, t: f32) -> LandAir {
        let f = |x: f32, y: f32| x + (y - x) * t;
        let c = |x: [f32; 3], y: [f32; 3]| Vec3::from(x).lerp(Vec3::from(y), t).into();
        LandAir {
            fog: c(self.fog, b.fog),
            haze: c(self.haze, b.haze),
            sky_horizon: c(self.sky_horizon, b.sky_horizon),
            sky_top: c(self.sky_top, b.sky_top),
            density: f(self.density, b.density),
            falloff: f(self.falloff, b.falloff),
            saturation: f(self.saturation, b.saturation),
            contrast: f(self.contrast, b.contrast),
            sun: c(self.sun, b.sun),
            shadow_tint: c(self.shadow_tint, b.shadow_tint),
            light_tint: c(self.light_tint, b.light_tint),
        }
    }

    /// The weighted average of `airs` (`weights` need not add up to 1);
    /// neutral when every weight is 0.
    pub fn mixed(airs: &[LandAir], weights: &[f32]) -> LandAir {
        let mut mix = LandAir::NEUTRAL;
        let mut total = 0.0;
        for (air, &w) in airs.iter().zip(weights) {
            if w <= 0.0 {
                continue;
            }
            total += w;
            mix = mix.lerp(air, w / total);
        }
        mix
    }
}

/// One number of a [`Look`] a person can move while tuning by eye (the
/// tuning panel, F10). Some are derived (the sun's height and heading are
/// angles of its direction; strengths scale a colour), so a step means
/// something to a person whatever the colour underneath.
pub struct Knob {
    pub name: &'static str,
    /// How much one key press moves it.
    pub step: f32,
    pub get: fn(&Look) -> f32,
    pub set: fn(&mut Look, f32),
}

/// The sun's height above the horizon, degrees.
fn sun_height(l: &Look) -> f32 {
    l.sun_dir().y.clamp(-1.0, 1.0).asin().to_degrees()
}

/// Which way the sun lies round the compass, degrees.
fn sun_heading(l: &Look) -> f32 {
    let d = l.sun_dir();
    d.x.atan2(-d.z).to_degrees()
}

/// Point the sun `height` degrees up and `heading` round.
fn aim_sun(l: &mut Look, height: f32, heading: f32) {
    let (h, a) = (height.clamp(2.0, 89.0).to_radians(), heading.to_radians());
    l.sun = [h.cos() * a.sin(), h.sin(), -h.cos() * a.cos()];
}

fn strength(c: [f32; 3]) -> f32 {
    Vec3::from(c).max_element()
}

fn scaled(c: [f32; 3], to: f32) -> [f32; 3] {
    let k = to.max(0.0) / strength(c).max(1e-4);
    c.map(|v| v * k)
}

/// The most the Warmth knob warms or cools the bright tint.
const MAX_WARMTH: f32 = 0.5;

/// A knob that is one plain number of the look, never below `min`.
macro_rules! field_knob {
    ($name:literal, $step:literal, $field:ident, $min:literal) => {
        Knob {
            name: $name,
            step: $step,
            get: |l| l.$field,
            set: |l, v| l.$field = v.max($min),
        }
    };
}

/// Every knob of the tuning panel, in the order it lists them.
pub const KNOBS: [Knob; 16] = [
    Knob {
        name: "Sun height (deg)",
        step: 2.0,
        get: sun_height,
        set: |l, v| aim_sun(l, v, sun_heading(l)),
    },
    Knob {
        name: "Sun heading (deg)",
        step: 5.0,
        get: sun_heading,
        set: |l, v| aim_sun(l, sun_height(l), v),
    },
    Knob {
        name: "Sun strength",
        step: 0.1,
        get: |l| strength(l.sun_color),
        set: |l, v| l.sun_color = scaled(l.sun_color, v),
    },
    Knob {
        name: "Sky light",
        step: 0.02,
        get: |l| strength(l.sky_ambient),
        set: |l, v| l.sky_ambient = scaled(l.sky_ambient, v),
    },
    field_knob!("Exposure", 0.02, exposure, 0.1),
    field_knob!("Contrast", 0.02, contrast, 0.2),
    field_knob!("Saturation", 0.03, saturation, 0.0),
    field_knob!("Vignette", 0.03, vignette, 0.0),
    field_knob!("Film grain", 0.005, grain, 0.0),
    field_knob!("Air thickness", 0.0002, fog_density, 0.0),
    field_knob!("Air hugs ground", 0.005, fog_falloff, 0.0),
    field_knob!("Rim light", 0.05, rim, 0.0),
    field_knob!("Leaf glow", 0.05, foliage_glow, 0.0),
    field_knob!("Cloud shadows", 0.05, cloud_shadows, 0.0),
    field_knob!("Valley shade", 0.05, ground_occlusion, 0.0),
    Knob {
        name: "Warmth",
        step: 0.01,
        // Red against blue in the bright end's tint.
        get: |l| l.light_tint[0] - l.light_tint[2],
        set: |l, v| {
            // Never so far that a tint goes negative.
            let v = v.clamp(-MAX_WARMTH, MAX_WARMTH);
            let mid = (l.light_tint[0] + l.light_tint[2]) / 2.0;
            l.light_tint[0] = mid + v / 2.0;
            l.light_tint[2] = mid - v / 2.0;
        },
    },
];

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
        assert_eq!(LookPreset::from_name("night"), Some(LookPreset::Night));
        assert_eq!(LookPreset::from_name("midnight"), None);
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
    fn the_moon_is_low_so_the_game_knows_it_is_dark() {
        // The ambient touches and lit windows read the sun's height; the
        // threshold is `ambient::touches::DUSK_AT`.
        assert!(LookPreset::Night.look().sun_dir().y < 0.25);
        assert!(LookPreset::Noon.look().sun_dir().y > 0.5);
    }

    #[test]
    fn night_is_dim_but_not_black() {
        let night = LookPreset::Night.look();
        let noon = LookPreset::Noon.look();
        let level = |l: &Look| l.sun_color.iter().sum::<f32>() * l.exposure;
        assert!(level(&night) < level(&noon) * 0.5);
        // The shade is not left black: the unlit side still reads.
        assert!(night.sky_ambient.iter().sum::<f32>() * night.exposure > 0.25);
    }

    #[test]
    fn weather_names_are_found_and_cycle() {
        assert_eq!(Weather::from_name("RAIN"), Some(Weather::Rain));
        assert_eq!(Weather::from_name("hail"), None);
        let mut w = Weather::Clear;
        for _ in 0..Weather::ALL.len() {
            w = w.next();
        }
        assert_eq!(w, Weather::Clear);
    }

    #[test]
    fn clear_skies_and_no_amount_change_nothing() {
        let look = LookPreset::Dusk.look();
        assert_eq!(look.in_weather(Weather::Clear, 1.0), look);
        for w in Weather::ALL {
            assert_eq!(look.in_weather(w, 0.0), look, "{}", w.name());
        }
    }

    #[test]
    fn weather_dims_the_sun_and_thickens_the_air_more_the_more_there_is() {
        for preset in LookPreset::ALL {
            let look = preset.look();
            for w in [Weather::Rain, Weather::Snow, Weather::Fog] {
                let half = look.in_weather(w, 0.5);
                let full = look.in_weather(w, 1.0);
                assert!(
                    half.sun_color[0] < look.sun_color[0],
                    "{} {}",
                    preset.name(),
                    w.name()
                );
                assert!(full.sun_color[0] < half.sun_color[0]);
                assert!(half.fog_density > look.fog_density);
                assert!(full.fog_density > half.fog_density);
                assert!(full.saturation < look.saturation);
                assert!(full.cloud_shadows <= 0.9 + 1e-6);
            }
        }
    }

    #[test]
    fn rain_greys_the_air_and_snow_whitens_it() {
        let look = LookPreset::GoldenHour.look();
        let spread = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
        let rain = look.in_weather(Weather::Rain, 1.0);
        assert!(spread(rain.haze) < spread(look.haze) * 0.6);
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let snow = look.in_weather(Weather::Snow, 1.0);
        assert!(luma(snow.sky_horizon) > luma(rain.sky_horizon));
        // Colours stay in range.
        for c in [rain.fog, snow.haze, snow.sky_top, rain.sky_horizon] {
            assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "{c:?}");
        }
    }

    #[test]
    fn fog_lies_low_and_rain_does_not_change_how_the_air_thins() {
        let look = LookPreset::Noon.look();
        assert!(look.in_weather(Weather::Fog, 1.0).fog_falloff > look.fog_falloff);
        assert_eq!(
            look.in_weather(Weather::Rain, 1.0).fog_falloff,
            look.fog_falloff
        );
    }

    #[test]
    fn a_neutral_air_changes_nothing_and_airs_mix_by_weight() {
        let look = LookPreset::Dusk.look();
        assert_eq!(look.in_air(&LandAir::NEUTRAL), look);
        let amber = LandAir {
            density: 2.0,
            fog: [0.2, 0.0, -0.2],
            ..LandAir::NEUTRAL
        };
        let half = LandAir::mixed(&[LandAir::NEUTRAL, amber], &[1.0, 1.0]);
        assert!((half.density - 1.5).abs() < 1e-6);
        assert!((half.fog[0] - 0.1).abs() < 1e-6);
        assert_eq!(LandAir::mixed(&[amber], &[0.0]), LandAir::NEUTRAL);
        // An air never pushes a colour out of range.
        let out = LookPreset::Noon.look().in_air(&LandAir {
            fog: [0.9, 0.9, -0.9],
            ..LandAir::NEUTRAL
        });
        assert!(out.fog.iter().all(|c| (0.0..=1.0).contains(c)));
    }

    #[test]
    fn clarity_drops_the_vignette_grain_and_heavy_grading_in_every_look() {
        for p in LookPreset::ALL {
            let clear = p.look().clear();
            assert_eq!((clear.vignette, clear.grain), (0.0, 0.0), "{}", p.name());
            assert!(clear.contrast <= CLEAR_CONTRAST, "{}", p.name());
            assert!((1.0..=CLEAR_SATURATION).contains(&clear.saturation));
            assert_eq!(clear.light_tint, [1.0; 3]);
            // The light itself is kept.
            assert_eq!(clear.sun, p.look().sun);
            assert_eq!(clear.fog, p.look().fog);
        }
    }

    #[test]
    fn knobs_move_what_they_name() {
        let look = LookPreset::GoldenHour.look();
        let none = [0.0; KNOBS.len()];
        assert_eq!(look.tuned(&none), look);
        for (i, knob) in KNOBS.iter().enumerate() {
            let mut d = none;
            d[i] = knob.step;
            let moved = look.tuned(&d);
            let (was, now) = ((knob.get)(&look), (knob.get)(&moved));
            assert!(
                (now - (was + knob.step)).abs() < 1e-3 * (1.0 + was.abs()),
                "{}: {was} → {now}",
                knob.name
            );
        }
    }

    #[test]
    fn the_sun_angles_read_back_what_was_aimed() {
        let mut look = LookPreset::Noon.look();
        aim_sun(&mut look, 30.0, 120.0);
        assert!((sun_height(&look) - 30.0).abs() < 1e-3);
        assert!((sun_heading(&look) - 120.0).abs() < 1e-3);
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
