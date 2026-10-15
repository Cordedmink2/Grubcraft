//! Bird's-eye RTS camera: fixed pitch, limited yaw, zoom by distance.
//! See docs/08-ui-ux.md#camera.

use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

const PITCH: f32 = 55.0 * std::f32::consts::PI / 180.0;
const FOV_Y: f32 = 35.0 * std::f32::consts::PI / 180.0;
const MAX_YAW: f32 = 45.0 * std::f32::consts::PI / 180.0;
pub const MIN_DISTANCE: f32 = 14.0;
pub const MAX_DISTANCE: f32 = 260.0;

pub struct RtsCamera {
    /// Ground point the camera looks at, in tiles.
    pub focus: Vec2,
    /// Height of the ground under the focus (smoothed by the caller).
    pub focus_height: f32,
    pub distance: f32,
    /// Rotation away from the default north-facing view, in radians.
    pub yaw: f32,
    /// The view Home returns to, and the middle of the turning range: north
    /// (zero), or towards the enemy for a player who starts in the north.
    pub home_yaw: f32,
    pub map_size: f32,
    pub aspect: f32,
}

impl RtsCamera {
    pub fn new(map_size: f32, focus: Vec2) -> RtsCamera {
        RtsCamera {
            focus,
            focus_height: 0.0,
            distance: 110.0,
            yaw: 0.0,
            home_yaw: 0.0,
            map_size,
            aspect: 16.0 / 9.0,
        }
    }

    fn offset_dir(&self) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        Vec3::new(s * PITCH.cos(), PITCH.sin(), c * PITCH.cos())
    }

    pub fn eye(&self) -> Vec3 {
        self.target() + self.offset_dir() * self.distance
    }

    fn target(&self) -> Vec3 {
        Vec3::new(self.focus.x, self.focus_height, self.focus.y)
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = look_at_mat4(self.eye(), self.target(), Vec3::Y);
        let proj = perspective(FOV_Y, self.aspect, 1.0, self.distance * 3.0 + 300.0);
        proj * view
    }

    /// Pan in screen-relative directions; `dx`/`dy` in screen fractions.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (s, c) = self.yaw.sin_cos();
        let right = Vec2::new(c, -s);
        let forward = Vec2::new(-s, -c);
        let scale = self.distance * 0.9;
        self.focus += (right * dx + forward * dy) * scale;
        self.focus = self.focus.clamp(Vec2::ZERO, Vec2::splat(self.map_size));
    }

    /// Multiply the distance by `factor` (< 1 zooms in).
    pub fn zoom(&mut self, factor: f32) {
        if factor.is_finite() {
            self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }

    pub fn rotate(&mut self, radians: f32) {
        if radians.is_finite() {
            self.yaw = (self.yaw + radians).clamp(self.home_yaw - MAX_YAW, self.home_yaw + MAX_YAW);
        }
    }

    pub fn reset_rotation(&mut self) {
        self.yaw = self.home_yaw;
    }

    /// Make "forward" on screen point along `forward` on the ground, and
    /// turn there.
    pub fn face(&mut self, forward: Vec2) {
        self.home_yaw = if forward.length_squared() > 0.0 {
            (-forward.x).atan2(-forward.y)
        } else {
            0.0
        };
        self.yaw = self.home_yaw;
    }

    /// World-space ray through a point given in normalised device coordinates.
    pub fn ray(&self, ndc: Vec2) -> (Vec3, Vec3) {
        let inv = self.view_proj().inverse();
        let near = inv * ndc.extend(0.0).extend(1.0);
        let far = inv * ndc.extend(1.0).extend(1.0);
        let near = near.xyz() / near.w;
        let far = far.xyz() / far.w;
        (near, (far - near).normalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_ray_hits_focus() {
        let cam = RtsCamera::new(256.0, Vec2::new(100.0, 120.0));
        let (o, d) = cam.ray(Vec2::ZERO);
        let t = -o.y / d.y;
        let hit = o + d * t;
        assert!(
            (hit.x - 100.0).abs() < 0.01 && (hit.z - 120.0).abs() < 0.01,
            "{hit}"
        );
    }

    #[test]
    fn zoom_and_yaw_are_clamped() {
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        cam.zoom(1000.0);
        assert_eq!(cam.distance, MAX_DISTANCE);
        cam.rotate(10.0);
        assert_eq!(cam.yaw, MAX_YAW);
        cam.zoom(f32::NAN);
        assert_eq!(cam.distance, MAX_DISTANCE);
    }

    #[test]
    fn facing_south_turns_the_view_round() {
        let mut cam = RtsCamera::new(256.0, Vec2::new(128.0, 128.0));
        cam.face(Vec2::Y);
        // The eye sits north of the focus, looking south.
        assert!(cam.eye().z < 128.0);
        // Panning "up" the screen moves the view south.
        cam.pan(0.0, 0.1);
        assert!(cam.focus.y > 128.0);
        cam.rotate(10.0);
        assert!((cam.yaw - cam.home_yaw - MAX_YAW).abs() < 1e-5);
        cam.reset_rotation();
        assert_eq!(cam.yaw, cam.home_yaw);
        cam.face(Vec2::NEG_Y);
        assert_eq!(cam.yaw, 0.0);
    }
}
