//! Point lights: lamps, torches and fires that light the ground and the
//! figures round them at night. A frame has at most [`MAX_LIGHTS`] of them,
//! the nearest ones the game picks; they are uniforms read by every lit
//! surface in `world.wgsl` (`local_lights`), so there is no extra pass and
//! no cost at all in daylight, when the list is empty. They do not cast
//! shadows. Render only: the game decides what is lit from what the viewer
//! sees.

use glam::Vec3;

/// The most lights in one frame. `world.wgsl` holds `2 * MAX_LIGHTS` vectors.
pub const MAX_LIGHTS: usize = 24;

/// One light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    /// Where it shines from (tiles, `y` up).
    pub pos: Vec3,
    /// How far it reaches, in tiles; the light falls to nothing there.
    pub radius: f32,
    /// Its colour times its strength, HDR (a surface facing it takes this
    /// much light at the middle; about 1.7 is a sunlit white).
    pub color: [f32; 3],
    /// Varies how it flickers, 0 to 255.
    pub seed: u8,
}

/// The lights as the shader reads them: for each, its position and radius,
/// then its colour and seed.
pub(crate) fn pack(lights: &[PointLight]) -> [[f32; 4]; 2 * MAX_LIGHTS] {
    let mut out = [[0.0; 4]; 2 * MAX_LIGHTS];
    for (i, l) in lights.iter().take(MAX_LIGHTS).enumerate() {
        out[2 * i] = [l.pos.x, l.pos.y, l.pos.z, l.radius.max(0.01)];
        out[2 * i + 1] = [l.color[0], l.color[1], l.color[2], f32::from(l.seed)];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_pack_in_pairs_and_the_extras_are_dropped() {
        let one = PointLight {
            pos: Vec3::new(1.0, 2.0, 3.0),
            radius: 4.0,
            color: [1.0, 0.5, 0.25],
            seed: 9,
        };
        let packed = pack(&[one; MAX_LIGHTS + 5]);
        assert_eq!(packed[0], [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(packed[1], [1.0, 0.5, 0.25, 9.0]);
        assert_eq!(packed[2 * MAX_LIGHTS - 1][3], 9.0);
        assert_eq!(pack(&[])[0], [0.0; 4]);
    }

    #[test]
    fn the_shader_holds_as_many_lights_as_the_renderer_sends() {
        let shader = include_str!("shaders/world.wgsl");
        assert!(
            shader.contains(&format!("lights: array<vec4<f32>, {}>", 2 * MAX_LIGHTS)),
            "world.wgsl's light array must hold 2 * MAX_LIGHTS vectors"
        );
    }
}
