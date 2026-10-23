//! The sun's shadow map: a depth image of the map as the sun sees it, over
//! the part of the map in view, drawn before each frame and read by every
//! sunlit surface.

use glam::camera::rh::{proj::directx::orthographic, view::look_at_mat4};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

/// Texels per side of the shadow map (8 MB at 16 bits).
pub const SHADOW_SIZE: u32 = 2048;
pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;
/// Pushes casters slightly away from the sun so lit surfaces don't shadow
/// themselves.
pub const SHADOW_BIAS: wgpu::DepthBiasState = wgpu::DepthBiasState {
    constant: 2,
    slope_scale: 2.0,
    clamp: 0.0,
};
/// Shadows reach this far from the camera, in tiles: past the far corners
/// of the view at the camera's widest zoom (260 tiles out at 55°, whose
/// far corners are about 390 tiles away).
pub const SHADOW_RANGE: f32 = 400.0;
/// The covered square's side is rounded up to this many tiles, so its
/// texel size changes in steps as the view zooms rather than every frame.
const SIDE_STEP: f32 = 16.0;
/// Shadows fade out over the outer part of the map past this fraction of
/// its half-width (`sunlight` in `shaders/world.wgsl`); the square is
/// padded so that what the camera sees stays inside it.
pub const SHADOW_FADE: f32 = 0.95;

/// The sun's view and projection, and the size of one shadow texel in
/// tiles, covering what `view_proj` shows of a map `map_size` tiles across
/// whose ground lies between 0 and `max_height`, out to `range` tiles from
/// the camera. `sun` points toward the sun.
pub fn sun_view_proj(
    view_proj: Mat4,
    sun: Vec3,
    map_size: f32,
    max_height: f32,
    range: f32,
) -> (Mat4, f32) {
    // The ground the view can show: along each corner's ray, from where it
    // comes down to the highest ground to where it meets the lowest (or the
    // shadow range, if nearer).
    let inv = view_proj.inverse();
    let at = |x: f32, y: f32, z: f32| {
        let p = inv * Vec4::new(x, y, z, 1.0);
        p.xyz() / p.w
    };
    let mut seen = [Vec3::ZERO; 8];
    for (i, (x, y)) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let near = at(x, y, 0.0);
        let ray = at(x, y, 1.0) - near;
        let mut end = (range / ray.length()).min(1.0);
        let mut start = 0.0;
        if ray.y < 0.0 {
            end = end.min(-near.y / ray.y);
            start = ((max_height - near.y) / ray.y).max(0.0);
        }
        let end = end.max(0.0);
        seen[i] = near + ray * start.min(end);
        seen[i + 4] = near + ray * end;
    }
    // A fixed orientation, so moving the view only slides the covered
    // square, and sliding it by whole texels keeps shadow edges still.
    let up = if sun.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let view = look_at_mat4(sun, Vec3::ZERO, up);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in seen {
        let v = view.transform_point3(p);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let extent = (hi.x - lo.x).max(hi.y - lo.y) / SHADOW_FADE;
    let side = (extent / SIDE_STEP).ceil().max(1.0) * SIDE_STEP;
    let texel = side / SHADOW_SIZE as f32;
    let snap = |v: f32| (v / texel).floor() * texel;
    let left = snap((lo.x + hi.x - side) * 0.5);
    let bottom = snap((lo.y + hi.y - side) * 0.5);
    // Depth covers the whole map, so hills outside the view still cast.
    let (mut near, mut far) = (f32::MAX, f32::MIN);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { 0.0 } else { map_size },
            if i & 2 == 0 { 0.0 } else { max_height },
            if i & 4 == 0 { 0.0 } else { map_size },
        );
        let d = -view.transform_point3(corner).z;
        near = near.min(d);
        far = far.max(d);
    }
    let proj = orthographic(
        left,
        left + side,
        bottom,
        bottom + side,
        near - 1.0,
        far + 1.0,
    );
    (proj * view, texel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::camera::rh::proj::directx::perspective;

    fn camera(focus: Vec3) -> Mat4 {
        let eye = focus + Vec3::new(0.0, 90.0, 63.0);
        perspective(0.6, 1.6, 1.0, 600.0) * look_at_mat4(eye, focus, Vec3::Y)
    }

    #[test]
    fn the_shadow_map_covers_the_view_and_the_map_depth() {
        let focus = Vec3::new(200.0, 0.0, 200.0);
        // Under every look, the low suns included.
        for preset in crate::LookPreset::ALL {
            let sun = preset.look().sun_dir();
            let (m, _) = sun_view_proj(camera(focus), sun, 512.0, 40.0, SHADOW_RANGE);
            for p in [
                focus,
                focus + Vec3::new(30.0, 0.0, 10.0),
                focus + Vec3::new(-30.0, 20.0, -20.0),
            ] {
                let c = m.project_point3(p);
                assert!(
                    c.x.abs() < 1.0 && c.y.abs() < 1.0,
                    "{} {p} → {c}",
                    preset.name()
                );
                assert!((0.0..1.0).contains(&c.z), "{} {p} → {c}", preset.name());
            }
            // Higher points are nearer the sun.
            let low = m.project_point3(focus).z;
            let high = m.project_point3(focus + Vec3::Y * 10.0).z;
            assert!(high < low, "{}", preset.name());
        }
    }

    #[test]
    fn the_widest_zoom_keeps_what_it_sees_clear_of_the_fade() {
        // The game camera at its widest: 260 tiles out at 55°, 35° high.
        let focus = Vec3::new(256.0, 0.0, 256.0);
        let (pitch, fov) = (55f32.to_radians(), 35f32.to_radians());
        let eye = focus + Vec3::new(0.0, pitch.sin(), pitch.cos()) * 260.0;
        let view_proj =
            perspective(fov, 16.0 / 9.0, 1.0, 1080.0) * look_at_mat4(eye, focus, Vec3::Y);
        let (m, _) = sun_view_proj(
            view_proj,
            crate::Look::default().sun_dir(),
            512.0,
            40.0,
            SHADOW_RANGE,
        );
        let inv = view_proj.inverse();
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            // Where the corner's ray meets the ground, and the highest ground.
            let near = inv.project_point3(Vec3::new(x, y, 0.0));
            let ray = inv.project_point3(Vec3::new(x, y, 1.0)) - near;
            for h in [0.0, 40.0] {
                let p = near + ray * ((h - near.y) / ray.y);
                let c = m.project_point3(p);
                assert!(c.x.abs().max(c.y.abs()) <= SHADOW_FADE + 1e-3, "{p} → {c}");
            }
        }
    }

    #[test]
    fn panning_moves_the_map_by_whole_texels() {
        let sun = crate::Look::default().sun_dir();
        let a = Vec3::new(200.0, 0.0, 200.0);
        let b = a + Vec3::new(3.37, 0.0, 1.91);
        let (ma, ta) = sun_view_proj(camera(a), sun, 512.0, 40.0, SHADOW_RANGE);
        let (mb, tb) = sun_view_proj(camera(b), sun, 512.0, 40.0, SHADOW_RANGE);
        assert_eq!(ta, tb);
        // A fixed world point lands on the same sub-texel spot in both.
        let p = Vec3::new(210.0, 5.0, 190.0);
        let texels = |m: Mat4| {
            let c = m.project_point3(p);
            (glam::Vec2::new(c.x, c.y) * 0.5 + 0.5) * SHADOW_SIZE as f32
        };
        let d = texels(ma) - texels(mb);
        assert!((d - d.round()).abs().max_element() < 1e-2, "{d}");
    }
}
