//! The two cameras: the bird's-eye RTS camera (a set pitch that the unit
//! viewer tilts, limited yaw, zoom by distance) and the ride-with-the-banner
//! camera that follows one regiment from just above head height. See
//! docs/08-ui-ux.md#camera.

use crate::buildings::ray_box;
use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

/// How steeply the bird's-eye camera looks down from its default distance
/// and further out.
const PITCH: f32 = 55.0 * std::f32::consts::PI / 180.0;
/// How steeply it looks down at its closest zoom: nearer the horizon, so
/// the scene reads as a diorama seen at eye level and the sky starts to
/// show. In between, the tilt follows the zoom.
const CLOSE_PITCH: f32 = 30.0 * std::f32::consts::PI / 180.0;
const FOV_Y: f32 = 35.0 * std::f32::consts::PI / 180.0;
const MAX_YAW: f32 = 45.0 * std::f32::consts::PI / 180.0;
pub const MIN_DISTANCE: f32 = 14.0;
pub const MAX_DISTANCE: f32 = 260.0;
/// Where the bird's-eye camera starts, and where it comes back to after a ride.
pub const DEFAULT_DISTANCE: f32 = 110.0;

/// Where the player is "standing" in the world, for sound: a ground position,
/// the direction of their right ear, and how far out they are zoomed (which
/// widens what they hear).
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub position: Vec2,
    pub right: Vec2,
    pub distance: f32,
}

#[derive(Clone, Debug)]
pub struct RtsCamera {
    /// Ground point the camera looks at, in tiles.
    pub focus: Vec2,
    /// Height of the ground under the focus (smoothed by the caller).
    pub focus_height: f32,
    pub distance: f32,
    /// How steeply the camera looks down, in radians. The battlefield view
    /// follows the zoom (`pitch_for`; zoom with `zoom` or `set_distance`);
    /// the unit viewer sets it itself.
    pub pitch: f32,
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
            distance: DEFAULT_DISTANCE,
            pitch: Self::pitch_for(DEFAULT_DISTANCE),
            yaw: 0.0,
            home_yaw: 0.0,
            map_size,
            aspect: 16.0 / 9.0,
        }
    }

    /// How steeply to look down from `distance`: `CLOSE_PITCH` at the
    /// closest zoom, easing out to `PITCH` at the default distance.
    fn pitch_for(distance: f32) -> f32 {
        let t = ((distance - MIN_DISTANCE) / (DEFAULT_DISTANCE - MIN_DISTANCE)).clamp(0.0, 1.0);
        CLOSE_PITCH + (PITCH - CLOSE_PITCH) * t * t * (3.0 - 2.0 * t)
    }

    /// Look down at the full battlefield angle whatever the zoom, for the
    /// showroom, which frames its models from the usual pitch.
    pub fn level_pitch(&mut self) {
        self.pitch = PITCH;
    }

    /// Move to `distance` from the focus (within the zoom range), tilting
    /// with it.
    pub fn set_distance(&mut self, distance: f32) {
        self.distance = distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.pitch = Self::pitch_for(self.distance);
    }

    fn offset_dir(&self) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        Vec3::new(s * self.pitch.cos(), self.pitch.sin(), c * self.pitch.cos())
    }

    /// The direction the camera looks.
    pub fn forward(&self) -> Vec3 {
        -self.offset_dir()
    }

    pub fn eye(&self) -> Vec3 {
        self.target() + self.offset_dir() * self.distance
    }

    fn target(&self) -> Vec3 {
        Vec3::new(self.focus.x, self.focus_height, self.focus.y)
    }

    pub fn view_proj(&self) -> Mat4 {
        let view = look_at_mat4(self.eye(), self.target(), Vec3::Y);
        // Close up (the unit viewer) the near plane comes in so a model's
        // nearest edge is not cut; from battlefield heights it stays at 1.
        let near = (self.distance * 0.1).clamp(0.05, 1.0);
        let proj = perspective(FOV_Y, self.aspect, near, self.distance * 3.0 + 300.0);
        proj * view
    }

    /// The directions that are right and up on the screen.
    pub fn screen_axes(&self) -> (Vec3, Vec3) {
        let back = self.offset_dir();
        let right = Vec3::Y.cross(back).normalize();
        (right, back.cross(right))
    }

    /// How many pixels tall something 1 tile tall shows, 1 tile from the
    /// eye, on a viewport `height` pixels tall (divide by the distance for
    /// anything further).
    pub fn pixel_scale(&self, height: f32) -> f32 {
        height / (2.0 * (FOV_Y * 0.5).tan())
    }

    /// How far from the focus the camera must be, at its current focus,
    /// height and turn, for every point in `points` to fall inside the
    /// middle `fill` of the window (1 is edge to edge).
    pub fn fit_distance(&self, points: impl IntoIterator<Item = Vec3>, fill: f32) -> f32 {
        let back = self.offset_dir();
        let (right, up) = self.screen_axes();
        let half = (FOV_Y * 0.5).tan() * fill;
        points.into_iter().fold(0.0, |need: f32, p| {
            // The point across the screen, up it, and how much nearer the
            // eye than the focus it lies.
            let q = p - self.target();
            let nearer = q.dot(back);
            need.max(q.dot(right).abs() / (half * self.aspect) + nearer)
                .max(q.dot(up).abs() / half + nearer)
        })
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
            self.set_distance(self.distance * factor);
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

    /// Where sounds are heard from: the ground point looked at, widening
    /// as the camera pulls back.
    pub fn listener(&self) -> Listener {
        Listener {
            position: self.focus,
            right: Vec2::new(self.yaw.cos(), -self.yaw.sin()),
            distance: self.distance,
        }
    }

    /// Come back from a ride: look at `focus` from the normal distance, turned
    /// as close as the turning range allows to the way the ride was looking
    /// (`looking`, on the ground), so the picture does not spin on the way out.
    pub fn resume_from_ride(&mut self, focus: Vec2, ground: f32, looking: Vec2) {
        self.focus = focus.clamp(Vec2::ZERO, Vec2::splat(self.map_size));
        self.focus_height = ground;
        self.set_distance(DEFAULT_DISTANCE);
        // The eye sits at `+(sin yaw, cos yaw)` from the focus, so it looks
        // along `-(sin yaw, cos yaw)`.
        self.yaw = if looking.length_squared() > 1e-6 {
            // Measured from home, so the clamp never takes the long way round.
            let from_home = wrap_angle((-looking.x).atan2(-looking.y) - self.home_yaw);
            self.home_yaw + from_home.clamp(-MAX_YAW, MAX_YAW)
        } else {
            self.home_yaw
        };
    }
}

/// Whichever camera is in use this frame: the bird's-eye one, or the ride
/// camera on a viewport of the given aspect ratio. Everything that must
/// follow the view (picking, culling, shadows, the minimap, sound) asks this
/// instead of knowing which one it is.
#[derive(Clone, Copy)]
pub enum ActiveCamera<'a> {
    Rts(&'a RtsCamera),
    Ride(&'a FollowCamera, f32),
}

impl ActiveCamera<'_> {
    pub fn view_proj(self) -> Mat4 {
        match self {
            Self::Rts(c) => c.view_proj(),
            Self::Ride(c, aspect) => c.view_proj(aspect),
        }
    }

    pub fn eye(self) -> Vec3 {
        match self {
            Self::Rts(c) => c.eye(),
            Self::Ride(c, _) => c.eye(),
        }
    }

    /// The direction the camera looks.
    pub fn forward(self) -> Vec3 {
        match self {
            Self::Rts(c) => c.forward(),
            Self::Ride(c, _) => c.forward(),
        }
    }

    /// World-space ray (origin on the near plane, unit direction) through a
    /// point given in normalised device coordinates.
    pub fn ray(self, ndc: Vec2) -> (Vec3, Vec3) {
        unproject(self.view_proj().inverse(), ndc)
    }

    /// Where sounds are heard from.
    pub fn listener(self) -> Listener {
        match self {
            Self::Rts(c) => c.listener(),
            Self::Ride(c, _) => c.listener(),
        }
    }

    /// How wide the view is, for sizing marks that must stay readable: the
    /// zoom distance (a ride is as close as the bird's-eye camera gets).
    pub fn zoom_distance(self) -> f32 {
        match self {
            Self::Rts(c) => c.distance,
            Self::Ride(..) => MIN_DISTANCE,
        }
    }

    pub fn pixel_scale(self, height: f32) -> f32 {
        match self {
            Self::Rts(c) => c.pixel_scale(height),
            Self::Ride(c, _) => c.pixel_scale(height),
        }
    }

    /// The height of the ground under the view, for working out where things
    /// at ground level are on screen.
    pub fn ground_plane(self) -> f32 {
        match self {
            Self::Rts(c) => c.focus_height,
            Self::Ride(c, _) => c.ground(),
        }
    }

    /// The ground point and zoom distance to draw the scenery for.
    pub fn scenery_view(self) -> (Vec2, f32) {
        match self {
            Self::Rts(c) => (c.focus, c.distance),
            Self::Ride(c, _) => c.scenery_view(),
        }
    }
}

/// The world-space ray through a point given in normalised device
/// coordinates, from the inverse of the camera's `view_proj` (invert once to
/// take several rays through one camera).
pub fn unproject(inv: Mat4, ndc: Vec2) -> (Vec3, Vec3) {
    let near = inv * ndc.extend(0.0).extend(1.0);
    let far = inv * ndc.extend(1.0).extend(1.0);
    let near = near.xyz() / near.w;
    let far = far.xyz() / far.w;
    (near, (far - near).normalize())
}

/// How far the eye floats above the ground under it, in tiles. Soldiers are
/// 1.1 to 1.6 tall, so this is a little over head height: low enough to feel
/// the press of the line, high enough to see over the ranks ahead.
const RIDE_EYE_HEIGHT: f32 = 2.5;
/// The eye never goes closer than this to the ground under it, however the
/// smoothing lags behind a ridge.
const RIDE_MIN_CLEARANCE: f32 = 0.6;
/// How far the eye hangs behind the block's rear rank, in tiles.
const RIDE_BEHIND_REAR: f32 = 2.5;
/// Looking this far below the horizon puts the regiment in the lower half of
/// the picture with the enemy line and the sky above it.
const RIDE_PITCH: f32 = 12.0 * std::f32::consts::PI / 180.0;
/// Wider than the bird's-eye lens: close up, it gives the feeling of speed
/// and of being surrounded.
const RIDE_FOV_Y: f32 = 55.0 * std::f32::consts::PI / 180.0;
/// Near and far planes. There is a horizon to see, so the far plane
/// reaches past the haze (by 1,200 tiles it hides 95% of the ground) and
/// past the biggest map's diagonal (896 tiles a side: about 1,270), so the
/// ground fades out rather than stopping at an edge.
const RIDE_NEAR: f32 = 0.5;
const RIDE_FAR: f32 = 1300.0;
/// How far from the eye the sun's shadow map reaches while riding, in
/// tiles. The bird's-eye camera spreads the map's texels over what it sees
/// (up to 400 tiles out); a low camera sees a long strip of ground, so
/// spreading them over all of it would blur the shadows of the soldiers
/// beside you. Past this the ground is simply lit.
pub const RIDE_SHADOW_RANGE: f32 = 100.0;
/// Easing speeds, per second (the view closes `1 - e^-rate` of the gap per
/// second). Position follows briskly; turning is lazier so a block pivoting
/// on the spot swings the view round rather than snapping it; height is the
/// same as the bird's-eye camera's.
const RIDE_MOVE_RATE: f32 = 4.0;
const RIDE_TURN_RATE: f32 = 2.5;
const RIDE_GROUND_RATE: f32 = 6.0;
/// Scenery for the ride is drawn round a point this far ahead of the eye, as
/// if the bird's-eye camera were this far out (trees to 70 + 1.6 times it).
const RIDE_SCENERY_AHEAD: f32 = 40.0;
const RIDE_SCENERY_DISTANCE: f32 = 60.0;

/// How far the eye hangs behind the block's centre: half its depth and the
/// gap behind the rear rank.
fn back_for(target: RideTarget) -> f32 {
    target.depth * 0.5 + RIDE_BEHIND_REAR
}

/// What the ride camera follows, as the renderer draws it this frame.
#[derive(Clone, Copy, Debug)]
pub struct RideTarget {
    /// Centre of the block, in tiles.
    pub centre: Vec2,
    /// Unit vector of the way the block faces.
    pub facing: Vec2,
    /// Front-to-rear depth of the block, in tiles.
    pub depth: f32,
}

/// A camera that rides behind one regiment. It keeps a smoothed copy of the
/// regiment's position and heading and hangs the eye behind that, so a block
/// that turns, shoves or gives ground moves the view gently instead of
/// jerking it. Normal play never touches it.
#[derive(Clone, Copy, Debug)]
pub struct FollowCamera {
    anchor: Vec2,
    /// Heading in the map plane, radians from +x towards +y (the sim's angle).
    heading: f32,
    /// Distance from the anchor back to the eye, in tiles.
    back: f32,
    /// Smoothed ground height under the eye.
    ground: f32,
    /// Lowest the eye may be: true ground under it plus the clearance.
    floor: f32,
}

impl FollowCamera {
    /// Start exactly on the target (no swoop in from somewhere else).
    pub fn new(target: RideTarget, ground_at: impl Fn(Vec2) -> f32) -> FollowCamera {
        let mut cam = FollowCamera {
            anchor: target.centre,
            heading: heading_of(target.facing),
            back: back_for(target),
            ground: 0.0,
            floor: 0.0,
        };
        let g = ground_at(cam.eye_xz());
        cam.ground = g;
        cam.floor = g + RIDE_MIN_CLEARANCE;
        cam
    }

    /// Ease towards `target` over `dt` seconds. Frame-rate independent: two
    /// half-length steps land in the same place as one whole one.
    pub fn update(&mut self, dt: f32, target: RideTarget, ground_at: impl Fn(Vec2) -> f32) {
        let ease = |rate: f32| 1.0 - (-dt.max(0.0) * rate).exp();
        self.anchor += (target.centre - self.anchor) * ease(RIDE_MOVE_RATE);
        let want = heading_of(target.facing);
        self.heading += wrap_angle(want - self.heading) * ease(RIDE_TURN_RATE);
        self.back += (back_for(target) - self.back) * ease(RIDE_MOVE_RATE);
        let g = ground_at(self.eye_xz());
        self.ground += (g - self.ground) * ease(RIDE_GROUND_RATE);
        self.floor = g + RIDE_MIN_CLEARANCE;
    }

    /// The smoothed point the view is built around (for coming back out).
    pub fn anchor(&self) -> Vec2 {
        self.anchor
    }

    /// Unit vector of the way the camera looks, in the map plane.
    pub fn forward_xz(&self) -> Vec2 {
        Vec2::from_angle(self.heading)
    }

    fn eye_xz(&self) -> Vec2 {
        self.anchor - self.forward_xz() * self.back
    }

    pub fn eye(&self) -> Vec3 {
        let xz = self.eye_xz();
        Vec3::new(xz.x, (self.ground + RIDE_EYE_HEIGHT).max(self.floor), xz.y)
    }

    /// How high the ground is under the eye (smoothed), for working out
    /// where things at ground level are on screen.
    pub fn ground(&self) -> f32 {
        self.ground
    }

    /// How many pixels tall something 1 tile tall shows, 1 tile from the
    /// eye, on a viewport `height` pixels tall (as `RtsCamera::pixel_scale`).
    pub fn pixel_scale(&self, height: f32) -> f32 {
        height / (2.0 * (RIDE_FOV_Y * 0.5).tan())
    }

    /// The ground point and "zoom distance" to draw the scenery for: trees
    /// are drawn round a point well ahead of the eye, out to the horizon's
    /// worth that a camera pulled back to `RIDE_SCENERY_DISTANCE` would show.
    pub fn scenery_view(&self) -> (Vec2, f32) {
        (
            self.eye_xz() + self.forward_xz() * RIDE_SCENERY_AHEAD,
            RIDE_SCENERY_DISTANCE,
        )
    }

    /// The direction the camera looks.
    pub fn forward(&self) -> Vec3 {
        let f = self.forward_xz();
        Vec3::new(
            f.x * RIDE_PITCH.cos(),
            -RIDE_PITCH.sin(),
            f.y * RIDE_PITCH.cos(),
        )
    }

    /// The view and projection matrix on a viewport of this `aspect` ratio.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let view = look_at_mat4(eye, eye + self.forward(), Vec3::Y);
        let proj = perspective(RIDE_FOV_Y, aspect, RIDE_NEAR, RIDE_FAR);
        proj * view
    }

    pub fn listener(&self) -> Listener {
        let f = self.forward_xz();
        Listener {
            position: self.eye_xz(),
            right: Vec2::new(-f.y, f.x),
            // The ride listens as if zoomed fully in: the fight is loud and
            // close.
            distance: MIN_DISTANCE,
        }
    }
}

/// Where a ray first meets a block of soldiers, as a distance along `dir`.
///
/// The block is an upright box: `half` is half its extent along (`facing`,
/// across it), in tiles, and it spans `low..high` in height. From a low
/// camera a ray aimed at a soldier lands on the ground far behind the block
/// (a ray 3 degrees below the horizon travels 18 tiles to fall one soldier's
/// height), so picking by ground point alone would miss it; this picks the
/// body instead.
pub fn ray_hits_block(
    origin: Vec3,
    dir: Vec3,
    centre: Vec2,
    facing: Vec2,
    half: Vec2,
    (low, high): (f32, f32),
) -> Option<f32> {
    // Into the block's own frame: x along its facing, z across it.
    let across_axis = Vec2::new(-facing.y, facing.x);
    let local = |v: Vec2| Vec2::new(v.dot(facing), v.dot(across_axis));
    let o = local(Vec2::new(origin.x, origin.z) - centre);
    let d = local(Vec2::new(dir.x, dir.z));
    ray_box(
        Vec3::new(o.x, origin.y, o.y),
        Vec3::new(d.x, dir.y, d.y),
        Vec3::new(-half.x, low, -half.y),
        Vec3::new(half.x, high, half.y),
    )
}

/// Heading angle of a facing vector (the sim's convention: 0 is +x, a quarter
/// turn is +y). A zero vector faces +x.
fn heading_of(facing: Vec2) -> f32 {
    if facing.length_squared() > 1e-6 {
        facing.y.atan2(facing.x)
    } else {
        0.0
    }
}

/// An angle difference folded into -pi..pi, so turns take the short way round.
fn wrap_angle(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    a - tau * (a / tau).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_axes_are_perpendicular_to_each_other_and_the_view() {
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        for (yaw, pitch) in [(0.0, 0.4), (1.0, 1.2), (-2.0, 0.05)] {
            cam.yaw = yaw;
            cam.pitch = pitch;
            let (right, up) = cam.screen_axes();
            let back = -cam.forward();
            assert!(right.dot(up).abs() < 1e-5);
            assert!(right.dot(back).abs() < 1e-5);
            assert!(up.dot(back).abs() < 1e-5);
            assert!(up.y > 0.0, "up is up");
        }
        // Turned to the default, right is east.
        cam.yaw = 0.0;
        assert!((cam.screen_axes().0 - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn centre_ray_hits_focus() {
        let cam = RtsCamera::new(256.0, Vec2::new(100.0, 120.0));
        let (o, d) = ActiveCamera::Rts(&cam).ray(Vec2::ZERO);
        let t = -o.y / d.y;
        let hit = o + d * t;
        assert!(
            (hit.x - 100.0).abs() < 0.01 && (hit.z - 120.0).abs() < 0.01,
            "{hit}"
        );
    }

    #[test]
    fn zooming_in_tilts_the_view_toward_the_horizon() {
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        assert_eq!(cam.pitch, PITCH);
        cam.zoom(1000.0);
        assert_eq!(cam.pitch, PITCH);
        let mut last = cam.pitch;
        for _ in 0..40 {
            cam.zoom(0.9);
            assert!(cam.pitch <= last + 1e-6);
            last = cam.pitch;
        }
        assert!((cam.pitch - CLOSE_PITCH).abs() < 1e-5);
        assert_eq!(cam.distance, MIN_DISTANCE);
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

    #[test]
    fn pixel_scale_gives_the_projected_size() {
        let cam = RtsCamera::new(256.0, Vec2::ZERO);
        let expected = 760.0 / (2.0 * 17.5_f32.to_radians().tan());
        let scale = cam.pixel_scale(760.0);
        assert!((scale - expected).abs() < 0.01, "{scale} vs {expected}");
    }

    /// The ray through `ndc` of a ride camera on a 1.6 aspect screen.
    fn ray(cam: &FollowCamera, ndc: Vec2) -> (Vec3, Vec3) {
        ActiveCamera::Ride(cam, 1.6).ray(ndc)
    }

    fn flat(_: Vec2) -> f32 {
        3.0
    }

    fn target(centre: Vec2, facing: Vec2) -> RideTarget {
        RideTarget {
            centre,
            facing,
            depth: 3.0,
        }
    }

    #[test]
    fn ride_eye_sits_behind_the_rear_rank_and_above_head_height() {
        let t = target(Vec2::new(100.0, 100.0), Vec2::X);
        let cam = FollowCamera::new(t, flat);
        let eye = cam.eye();
        // Facing +x: behind is smaller x, same z. Half the depth plus the gap.
        assert!(
            (eye.x - (100.0 - 1.5 - RIDE_BEHIND_REAR)).abs() < 1e-4,
            "{eye}"
        );
        assert!((eye.z - 100.0).abs() < 1e-4);
        assert!((eye.y - (3.0 + RIDE_EYE_HEIGHT)).abs() < 1e-4);
        const { assert!(RIDE_EYE_HEIGHT > 1.6, "above the tallest soldier's head") };
    }

    #[test]
    fn ride_eye_stays_above_ground_even_on_a_sudden_ridge() {
        let mut cam = FollowCamera::new(target(Vec2::new(50.0, 50.0), Vec2::X), flat);
        // The ground under the eye jumps up 10 tiles; the smoothed height has
        // not caught up yet, but the eye must not be under the terrain.
        let ridge = |_: Vec2| 13.0;
        cam.update(0.016, target(Vec2::new(50.0, 50.0), Vec2::X), ridge);
        assert!(
            cam.eye().y >= 13.0 + RIDE_MIN_CLEARANCE - 1e-4,
            "{}",
            cam.eye()
        );
        // And it keeps the clearance while it settles.
        for _ in 0..600 {
            cam.update(0.016, target(Vec2::new(50.0, 50.0), Vec2::X), ridge);
            assert!(cam.eye().y >= 13.0 + RIDE_MIN_CLEARANCE - 1e-4);
        }
        assert!((cam.eye().y - (13.0 + RIDE_EYE_HEIGHT)).abs() < 0.01);
    }

    #[test]
    fn ride_follows_a_marching_block_and_converges() {
        let mut cam = FollowCamera::new(target(Vec2::ZERO, Vec2::Y), flat);
        let goal = target(Vec2::new(30.0, -20.0), Vec2::new(1.0, 1.0).normalize());
        for _ in 0..600 {
            cam.update(0.016, goal, flat);
        }
        assert!(cam.anchor().distance(goal.centre) < 0.01);
        assert!(cam.forward_xz().distance(goal.facing) < 0.01);
    }

    #[test]
    fn ride_damping_does_not_depend_on_frame_rate() {
        let goal = target(Vec2::new(10.0, 5.0), Vec2::new(0.0, -1.0));
        let start = target(Vec2::ZERO, Vec2::X);
        let mut coarse = FollowCamera::new(start, flat);
        let mut fine = FollowCamera::new(start, flat);
        coarse.update(0.2, goal, flat);
        for _ in 0..20 {
            fine.update(0.01, goal, flat);
        }
        assert!(coarse.anchor().distance(fine.anchor()) < 1e-3);
        assert!(coarse.forward_xz().distance(fine.forward_xz()) < 1e-3);
    }

    #[test]
    fn ride_turns_the_short_way_round() {
        // From facing just past -x (heading near +pi) to just before it
        // (heading near -pi) is a tiny turn, not nearly a full circle.
        let a = Vec2::from_angle(3.0);
        let b = Vec2::from_angle(-3.0);
        let mut cam = FollowCamera::new(target(Vec2::ZERO, a), flat);
        cam.update(0.016, target(Vec2::ZERO, b), flat);
        assert!(cam.forward_xz().dot(a) > 0.99, "{}", cam.forward_xz());
    }

    #[test]
    fn ride_ray_through_screen_centre_hits_the_ground_ahead() {
        let t = target(Vec2::new(100.0, 100.0), Vec2::X);
        let cam = FollowCamera::new(t, flat);
        let (o, d) = ray(&cam, Vec2::ZERO);
        assert!(d.y < 0.0, "looks down");
        let hit = o + d * ((3.0 - o.y) / d.y);
        // In front of the block (further along +x than its centre), on its line.
        assert!(hit.x > 100.0, "{hit}");
        assert!((hit.z - 100.0).abs() < 0.01);
        // About 12 degrees down from 2 tiles up: a bit under 10 tiles away.
        let ahead = hit.x - cam.eye().x;
        assert!(
            (ahead - RIDE_EYE_HEIGHT / RIDE_PITCH.tan()).abs() < 0.05,
            "{ahead}"
        );
    }

    #[test]
    fn ride_ray_right_of_centre_lands_to_the_blocks_right() {
        let cam = FollowCamera::new(target(Vec2::new(100.0, 100.0), Vec2::X), flat);
        let (o, d) = ray(&cam, Vec2::new(0.5, -0.3));
        let hit = o + d * ((3.0 - o.y) / d.y);
        // Facing +x with +y up, +z is the right-hand side.
        assert!(hit.z > 100.0, "{hit}");
    }

    #[test]
    fn ride_listener_is_the_eye_with_the_right_ear_to_the_right() {
        let cam = FollowCamera::new(target(Vec2::new(100.0, 100.0), Vec2::X), flat);
        let l = cam.listener();
        assert!((l.position.x - cam.eye().x).abs() < 1e-4);
        assert!(l.right.distance(Vec2::Y) < 1e-4, "{}", l.right);
        assert_eq!(l.distance, MIN_DISTANCE);
        // Same convention as the bird's-eye camera: looking north, right is +x.
        let rts = RtsCamera::new(256.0, Vec2::ZERO).listener();
        let north = FollowCamera::new(target(Vec2::ZERO, Vec2::new(0.0, -1.0)), flat);
        assert!(north.listener().right.distance(rts.right) < 1e-4);
    }

    #[test]
    fn coming_back_looks_the_way_the_ride_did_within_the_yaw_limit() {
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        // Riding north: the normal view is already north-up.
        cam.resume_from_ride(Vec2::new(40.0, 40.0), 2.0, Vec2::new(0.0, -1.0));
        assert!(cam.yaw.abs() < 1e-5);
        assert_eq!(cam.distance, DEFAULT_DISTANCE);
        assert_eq!(cam.focus, Vec2::new(40.0, 40.0));
        // Riding east is further than the limit: it stops at the limit and
        // actually looks east-ish.
        cam.resume_from_ride(Vec2::new(40.0, 40.0), 2.0, Vec2::X);
        assert!((cam.yaw.abs() - MAX_YAW).abs() < 1e-5);
        let (_, d) = ActiveCamera::Rts(&cam).ray(Vec2::ZERO);
        assert!(d.x > 0.0, "{d}");
        // Riding south cannot be matched; the yaw still stays in range.
        cam.resume_from_ride(Vec2::new(40.0, 40.0), 2.0, Vec2::Y);
        assert!(cam.yaw.abs() <= MAX_YAW + 1e-6);
        // A pan clamps the focus to the map.
        cam.resume_from_ride(Vec2::new(-5.0, 999.0), 0.0, Vec2::ZERO);
        assert_eq!(cam.focus, Vec2::new(0.0, 256.0));
        assert_eq!(cam.yaw, 0.0);
    }

    #[test]
    fn a_low_ray_picks_the_block_it_passes_through_not_the_ground_behind() {
        let cam = FollowCamera::new(target(Vec2::new(100.0, 100.0), Vec2::X), flat);
        let (o, d) = ray(&cam, Vec2::ZERO);
        let ground_hit = o + d * ((3.0 - o.y) / d.y);
        // A block a few tiles ahead of the eye, standing 3..4.95 high. The ray
        // goes through its body well before it reaches the ground.
        let centre = Vec2::new(o.x + 4.0, o.z);
        let hit = |c: Vec2| ray_hits_block(o, d, c, Vec2::X, Vec2::new(1.0, 2.0), (3.0, 4.95));
        assert!(hit(centre).is_some());
        // Picking by the ground point would have been more than the pick
        // radius away from the block's edge.
        assert!(ground_hit.x - (centre.x + 1.0) > 1.5, "{ground_hit}");
        // A block off to the side is missed; one behind the camera is missed.
        assert!(hit(centre + Vec2::new(0.0, 20.0)).is_none());
        assert!(hit(Vec2::new(o.x - 30.0, o.z)).is_none());
    }

    #[test]
    fn ray_block_hit_distance_is_where_the_ray_enters_the_box() {
        // Straight along +x at mid height into a 4 by 4 block centred 10 away.
        let t = ray_hits_block(
            Vec3::new(0.0, 3.5, 0.0),
            Vec3::X,
            Vec2::new(10.0, 0.0),
            Vec2::X,
            Vec2::new(2.0, 2.0),
            (3.0, 4.0),
        );
        assert!((t.unwrap() - 8.0).abs() < 1e-4);
        // Too high: passes over.
        let over = ray_hits_block(
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::X,
            Vec2::new(10.0, 0.0),
            Vec2::X,
            Vec2::new(2.0, 2.0),
            (3.0, 4.0),
        );
        assert!(over.is_none());
    }

    #[test]
    fn ride_view_reaches_across_the_map() {
        // A point on the far side of the map (about 360 tiles away) is
        // inside the clip volume at the screen centre height.
        let cam = FollowCamera::new(target(Vec2::new(10.0, 128.0), Vec2::X), flat);
        let p = cam.view_proj(1.6) * glam::Vec4::new(250.0, 3.0, 128.0, 1.0);
        let ndc = p.xyz() / p.w;
        assert!(ndc.z > 0.0 && ndc.z < 1.0, "{ndc}");
    }
}
