//! The unit viewer (main menu, "Unit viewer"): every unit and hero of the
//! four factions, one at a time on a plinth, under an orbit camera. The
//! panels are `gc_ui::viewer`; this module owns what is behind them: the
//! catalog of units written from the rosters at build time, the model each
//! one is drawn with, the camera (turn, tilt, zoom, move), and the pose.
//! The maths is plain functions, tested below.

use gc_assets::model::{ClipKind, Models, POSITION_SCALE};
use gc_render::BoxInstance;
use gc_ui::viewer as panel;
use glam::{Vec2, Vec3};

use crate::camera::RtsCamera;
use crate::models::Action;
use crate::showroom::{self, Clip, FACING};

/// One unit or hero, as the build script wrote it from `data/rosters`.
pub struct CatalogEntry {
    /// Index into `gc_data::Faction::ALL`.
    pub faction: u8,
    pub name: &'static str,
    pub kind: &'static str,
    pub tag: &'static str,
    pub flying: bool,
    pub facts: &'static [(&'static str, &'static str)],
    pub abilities: &'static [(&'static str, &'static str, &'static str)],
}

/// Every unit and hero, factions in `Faction` order.
pub static CATALOG: &[CatalogEntry] = include!(concat!(env!("OUT_DIR"), "/viewer_catalog.rs"));

/// The animation buttons, in the order the panel lists them: label, the
/// clip a model must have, and what to show.
const CLIPS: [(&str, ClipKind, Clip); 10] = [
    ("Idle", ClipKind::Idle, Clip::Play(Action::Idle)),
    ("Walk", ClipKind::Walk, Clip::Play(Action::Walk)),
    ("Run", ClipKind::Run, Clip::Play(Action::Run)),
    ("Attack", ClipKind::Attack, Clip::Play(Action::Attack)),
    ("Attack 2", ClipKind::Attack2, Clip::Play(Action::Attack2)),
    ("Shoot", ClipKind::Shoot, Clip::Play(Action::Shoot)),
    ("Cheer", ClipKind::Cheer, Clip::Play(Action::Cheer)),
    ("Hit", ClipKind::Hit, Clip::Play(Action::Hit)),
    ("Block", ClipKind::Block, Clip::Play(Action::Block)),
    ("Fall", ClipKind::Die, Clip::Die),
];

/// The view opens this steep (radians): low enough to read a silhouette,
/// high enough to see a head and shoulders and the ground.
const START_PITCH: f32 = 0.38;
const MIN_PITCH: f32 = 0.03;
const MAX_PITCH: f32 = 1.5;
/// The model fills this much of the window at the default zoom.
const FILL: f32 = 0.72;
/// How far in and out the wheel goes, as multiples of the default distance,
/// and never nearer than this many tiles.
const ZOOM_RANGE: (f32, f32) = (0.3, 4.0);
const NEAREST: f32 = 0.8;
/// Radians of turn and tilt for a drag across the whole window.
const TURN_PER_WINDOW: f32 = 2.0 * std::f32::consts::PI;
const TILT_PER_WINDOW: f32 = 2.0;
/// A flier is framed with this many extra heights above it, so it fits
/// wherever its clips lift it.
const FLIER_HEADROOM: f32 = 1.2;
/// The plinth is this much wider than the model's reach, at least this wide
/// (half-width), and this thick.
const PLINTH_MARGIN: f32 = 0.25;
const PLINTH_MIN_HALF: f32 = 0.7;
const PLINTH_HEIGHT: f32 = 0.12;
const PLINTH_COLOR: [u8; 4] = [92, 88, 82, 0];
const PLINTH_FRONT_COLOR: [u8; 4] = [150, 138, 104, 0];
/// The share of a model's vertices its framing and plinth must hold.
const BODY_SHARE: f32 = 0.97;
/// The flat patch the viewer looks for, in tiles from its middle.
const PATCH: f32 = 3.0;

/// How a model is drawn: its place in the pack, how far it reaches and how
/// tall it stands, and the clips behind the panel's animation buttons.
struct Look {
    model: usize,
    /// Furthest a vertex is from the model's middle, on the ground plane.
    reach: f32,
    height: f32,
    flying: bool,
    /// The clip each of the entry's animation buttons shows.
    clips: Vec<Clip>,
}

/// The panel's entries, and the look of each (none for a unit with no model
/// yet).
fn entries(models: &Models) -> (Vec<panel::Entry>, Vec<Option<Look>>) {
    let mut shown = Vec::with_capacity(CATALOG.len());
    let mut looks = Vec::with_capacity(CATALOG.len());
    for c in CATALOG {
        let look = models.by_name(c.name).map(|model| {
            let (reach, height) = extent(models, model);
            // The animations the model has, in the panel's order.
            let clips: Vec<(&'static str, Clip)> = CLIPS
                .iter()
                .filter(|(_, kind, _)| models.clip(model, *kind).is_some())
                .map(|&(label, _, clip)| (label, clip))
                .collect();
            (
                Look {
                    model,
                    reach,
                    height,
                    flying: c.flying,
                    clips: clips.iter().map(|&(_, clip)| clip).collect(),
                },
                clips.iter().map(|&(label, _)| label).collect::<Vec<_>>(),
            )
        });
        let (look, labels) = match look {
            Some((look, labels)) => (Some(look), labels),
            None => (None, Vec::new()),
        };
        shown.push(panel::Entry {
            faction: usize::from(c.faction),
            name: c.name.to_string(),
            kind: c.kind.to_string(),
            tag: c.tag.to_string(),
            facts: c
                .facts
                .iter()
                .map(|&(l, v)| (l.to_string(), v.to_string()))
                .collect(),
            abilities: c
                .abilities
                .iter()
                .map(|&(n, h, e)| (n.to_string(), h.to_string(), e.to_string()))
                .collect(),
            clips: labels,
        });
        looks.push(look);
    }
    (shown, looks)
}

/// How far the body of a model's rest pose reaches from its middle across
/// the ground, and how tall it stands, in tiles. A spear or a banner pole
/// can poke out much further than the body; most of the vertices are the
/// body, so the reach is where `BODY_SHARE` of them lie inside.
fn extent(models: &Models, model: usize) -> (f32, f32) {
    let r = &models.models[model];
    let verts =
        &models.vertices[r.first_vertex as usize..(r.first_vertex + r.vertex_count) as usize];
    let mut out: Vec<f32> = Vec::with_capacity(verts.len());
    let mut height = 0.0_f32;
    for v in verts {
        let [x, y, z] = v.pos.map(|p| f32::from(p) / POSITION_SCALE);
        out.push(x.hypot(z));
        height = height.max(y);
    }
    let at = ((out.len() as f32 * BODY_SHARE) as usize).min(out.len().saturating_sub(1));
    let reach = if out.is_empty() {
        0.0
    } else {
        *out.select_nth_unstable_by(at, f32::total_cmp).1
    };
    (reach.max(0.25), height.max(r.height).max(0.25))
}

/// The flattest patch of ground near the middle of a `tiles`-wide map, by the
/// `ground` height at a point: the model stands on it, so the plinth does
/// not sink into a slope.
fn flat_spot(tiles: f32, ground: impl Fn(Vec2) -> f32) -> Vec2 {
    let middle = Vec2::splat(tiles * 0.5);
    let span = (tiles * 0.3).max(PATCH + 1.0);
    let mut best = (f32::INFINITY, middle);
    let steps = 12;
    for i in 0..=steps {
        for j in 0..=steps {
            let at = middle + Vec2::new(i as f32 - 6.0, j as f32 - 6.0) * (span / steps as f32);
            let heights = [
                ground(at),
                ground(at + Vec2::new(PATCH, 0.0)),
                ground(at - Vec2::new(PATCH, 0.0)),
                ground(at + Vec2::new(0.0, PATCH)),
                ground(at - Vec2::new(0.0, PATCH)),
            ];
            let (low, high) = heights
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &g| {
                    (l.min(g), h.max(g))
                });
            // Flatter first; near the middle breaks a tie.
            let cost = (high - low) + 1e-4 * (at - middle).length();
            if cost < best.0 {
                best = (cost, at);
            }
        }
    }
    best.1
}

/// The turn, tilt, zoom and shift of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    /// A multiple of the default distance for the model on show.
    zoom: f32,
    /// How far the view has been moved from the model, in tiles: across the
    /// screen and up it.
    shift: Vec2,
}

impl Orbit {
    const START: Orbit = Orbit {
        yaw: 0.0,
        pitch: START_PITCH,
        zoom: 1.0,
        shift: Vec2::ZERO,
    };
}

/// A mouse button held on the model: what moving the mouse does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Turn,
    Move,
}

pub struct Viewer {
    pub panel: panel::Viewer,
    looks: Vec<Option<Look>>,
    /// Where the plinth stands, and the ground height there.
    spot: Vec2,
    ground: f32,
    orbit: Orbit,
    /// What the held mouse button does, where the mouse was, and which
    /// button it is (0 left, 1 right, 2 middle).
    drag: Option<(Drag, (f64, f64), usize)>,
    /// The camera before the viewer took it, given back on leaving.
    saved: RtsCamera,
    /// What the clock last restarted for: the entry and the animation held.
    shown: (usize, Option<usize>),
    /// Seconds the animation has played.
    time: f32,
}

impl Viewer {
    /// Open the viewer on `models`, standing on the flattest ground the
    /// `ground` function finds in a map `tiles` wide; `camera` is kept to
    /// be given back.
    pub fn new(
        models: &Models,
        tiles: f32,
        ground: impl Fn(Vec2) -> f32,
        camera: &RtsCamera,
    ) -> Viewer {
        let (shown, looks) = entries(models);
        let spot = flat_spot(tiles, &ground);
        let panel = panel::Viewer::new(shown);
        let current = panel.selected_index();
        Viewer {
            looks,
            ground: ground(spot),
            spot,
            orbit: Orbit::START,
            drag: None,
            saved: camera.clone(),
            shown: (current, None),
            time: 0.0,
            panel,
        }
    }

    /// The camera as it was before the viewer opened.
    pub fn saved_camera(&self) -> &RtsCamera {
        &self.saved
    }

    fn look(&self) -> Option<&Look> {
        self.looks.get(self.panel.selected_index())?.as_ref()
    }

    /// Tilt, turn and move the view back to where it starts.
    pub fn reset_view(&mut self) {
        self.orbit = Orbit::START;
    }

    /// The wheel or a pinch: `factor` below 1 comes closer.
    pub fn zoom(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.orbit.zoom = (self.orbit.zoom * factor).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        }
    }

    /// Turn the view by `radians` (a rotation gesture).
    pub fn turn(&mut self, radians: f32) {
        if radians.is_finite() {
            self.orbit.yaw += radians;
        }
    }

    /// A mouse button went down at `at` (window pixels): turn the model with
    /// the left button, move the view with the right or middle one, or with
    /// the left and Shift.
    pub fn press(&mut self, button: usize, shift: bool, at: (f64, f64)) {
        let drag = if button == 0 && !shift {
            Drag::Turn
        } else {
            Drag::Move
        };
        self.drag = Some((drag, at, button));
    }

    /// A mouse button came up: ends the drag it began, if it did.
    pub fn release(&mut self, button: usize) {
        if self.drag.is_some_and(|(_, _, b)| b == button) {
            self.drag = None;
        }
    }

    /// The mouse moved to `at` in a window of `size` pixels.
    pub fn drag_to(&mut self, at: (f64, f64), size: (u32, u32)) {
        let Some((drag, from, button)) = self.drag else {
            return;
        };
        self.drag = Some((drag, at, button));
        let dx = ((at.0 - from.0) / f64::from(size.0.max(1))) as f32;
        let dy = ((at.1 - from.1) / f64::from(size.1.max(1))) as f32;
        match drag {
            // Grab the model: drag right and its front comes round to the
            // right, drag down and its top tilts towards you.
            Drag::Turn => {
                self.orbit.yaw -= dx * TURN_PER_WINDOW;
                self.orbit.pitch =
                    (self.orbit.pitch + dy * TILT_PER_WINDOW).clamp(MIN_PITCH, MAX_PITCH);
            }
            Drag::Move => {
                // The view moves the other way, a window's height of drag
                // for as far as the view is wide at the model.
                let reach = self.look().map_or(1.0, |l| l.reach.max(l.height));
                let scale = reach * 2.0 * self.orbit.zoom;
                self.orbit.shift += Vec2::new(-dx * scale, dy * scale);
                let limit = reach * 1.5;
                self.orbit.shift.x = self.orbit.shift.x.clamp(-limit, limit);
                self.orbit.shift.y = self.orbit.shift.y.clamp(-limit, limit);
            }
        }
    }

    /// Advance the animation, and hold `camera` on the model.
    pub fn update(&mut self, dt: f32, camera: &mut RtsCamera) {
        let now = (self.panel.selected_index(), self.panel.clip());
        if now != self.shown {
            // A new unit or animation starts from its first frame.
            self.shown = now;
            self.time = 0.0;
        } else {
            self.time += dt;
        }
        let (reach, height) = self.look().map_or((1.0, 1.8), |l| {
            (
                l.reach,
                l.height * if l.flying { 1.0 + FLIER_HEADROOM } else { 1.0 },
            )
        });
        self.aim(camera, reach, height);
    }

    /// Put `camera` where the orbit says, for a model that reaches `reach`
    /// tiles from its middle and stands `height` tiles high (the space it
    /// needs, with headroom).
    fn aim(&self, camera: &mut RtsCamera, reach: f32, height: f32) {
        let o = self.orbit;
        camera.pitch = o.pitch;
        camera.yaw = o.yaw;
        camera.home_yaw = 0.0;
        // Where the view looks: the model's middle, moved by the shift
        // along the screen's right and up.
        let (right, up) = camera.screen_axes();
        let target = Vec3::new(self.spot.x, self.ground + height * 0.5, self.spot.y)
            + right * o.shift.x
            + up * o.shift.y;
        camera.focus = Vec2::new(target.x, target.z);
        camera.focus_height = target.y;
        // The default distance fits the whole space the model needs.
        let corners = (0..4).flat_map(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.5;
            let (s, c) = a.sin_cos();
            [0.0, height].map(move |y| {
                Vec3::new(
                    self.spot.x + s * reach,
                    self.ground + y,
                    self.spot.y + c * reach,
                )
            })
        });
        let base = RtsCamera {
            focus: Vec2::new(self.spot.x, self.spot.y),
            focus_height: self.ground + height * 0.5,
            pitch: START_PITCH,
            yaw: 0.0,
            ..camera.clone()
        }
        .fit_distance(corners, FILL);
        camera.distance = (base * o.zoom).max(NEAREST);
    }

    /// Stand the model on show in this frame's drawing, on its plinth.
    pub fn push(&self, art: &mut crate::Art, instances: &mut Vec<BoxInstance>, color: [u8; 3]) {
        let Some(look) = self.look() else {
            return;
        };
        let half = (look.reach + PLINTH_MARGIN).max(PLINTH_MIN_HALF);
        instances.push(BoxInstance {
            pos: self.spot.to_array(),
            facing: FACING.to_array(),
            size: [half * 2.0, PLINTH_HEIGHT, half * 2.0],
            color: PLINTH_COLOR,
            lift: 0.0,
        });
        // A pale strip along the front edge, so the way it faces shows from
        // any side.
        instances.push(BoxInstance {
            pos: (self.spot + FACING * (half - 0.08)).to_array(),
            facing: FACING.to_array(),
            size: [half * 2.0, PLINTH_HEIGHT + 0.02, 0.16],
            color: PLINTH_FRONT_COLOR,
            lift: 0.0,
        });
        let hold = self.panel.clip().and_then(|k| look.clips.get(k)).copied();
        let Some((pose, flat)) = showroom::pose(&art.clips, look.model, hold, self.time) else {
            return;
        };
        showroom::stand(
            art,
            look.model,
            self.spot,
            self.ground,
            PLINTH_HEIGHT,
            (pose, flat),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> Models<'static> {
        crate::models::load().unwrap()
    }

    fn viewer() -> Viewer {
        let cam = RtsCamera::new(256.0, Vec2::new(10.0, 10.0));
        Viewer::new(&models(), 256.0, |_| 5.0, &cam)
    }

    #[test]
    fn the_catalog_covers_every_roster_unit_with_text_to_show() {
        assert!(CATALOG.len() >= 55, "{}", CATALOG.len());
        for f in 0..4 {
            assert!(CATALOG.iter().any(|c| c.faction == f), "faction {f}");
        }
        for c in CATALOG {
            assert!(!c.facts.is_empty(), "{}", c.name);
            assert!(!c.kind.is_empty() && !c.tag.is_empty(), "{}", c.name);
        }
    }

    #[test]
    fn models_are_found_by_name_with_the_clips_they_have() {
        let m = models();
        let (shown, looks) = entries(&m);
        assert_eq!(shown.len(), CATALOG.len());
        assert_eq!(looks.len(), CATALOG.len());
        let at = |name: &str| CATALOG.iter().position(|c| c.name == name).unwrap();
        // The Yari Ashigaru and the Ronin have models; the Brood's untouched
        // units do not.
        let yari = &shown[at("Yari Ashigaru")];
        assert!(yari.clips.contains(&"Idle") && yari.clips.contains(&"Fall"));
        let look = looks[at("Yari Ashigaru")].as_ref().unwrap();
        assert_eq!(look.clips.len(), yari.clips.len());
        assert!(look.reach > 0.2 && look.height > 0.5);
        assert!(looks[at("War Kite")].as_ref().unwrap().flying);
        let without: Vec<&str> = CATALOG
            .iter()
            .zip(&looks)
            .filter(|(_, l)| l.is_none())
            .map(|(c, _)| c.name)
            .collect();
        assert!(!without.is_empty());
        for (e, l) in shown.iter().zip(&looks) {
            assert_eq!(
                e.clips.is_empty(),
                l.is_none() || l.as_ref().unwrap().clips.is_empty()
            );
        }
        // Every model in the roster rows is also a model the pack has: the
        // rows with a look are exactly the names the pack knows.
        for (c, l) in CATALOG.iter().zip(&looks) {
            assert_eq!(l.is_some(), m.by_name(c.name).is_some(), "{}", c.name);
        }
    }

    #[test]
    fn the_flat_spot_avoids_a_slope() {
        // Flat on the east half of a ramp that climbs 1 tile per tile on the
        // west half.
        let ground = |p: Vec2| if p.x < 128.0 { p.x } else { 128.0 };
        let spot = flat_spot(256.0, ground);
        for d in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
            assert!(
                (ground(spot + d * PATCH) - ground(spot)).abs() < 1e-3,
                "{spot}"
            );
        }
        // On level ground it stays at the middle.
        let level = flat_spot(256.0, |_| 3.0);
        assert!((level - Vec2::splat(128.0)).length() < 1e-3, "{level}");
    }

    /// Where a point lands on screen, in -1..1.
    fn on_screen(cam: &RtsCamera, p: Vec3) -> (f32, f32) {
        let v = cam.view_proj() * p.extend(1.0);
        (v.x / v.w, v.y / v.w)
    }

    #[test]
    fn the_default_view_fits_the_model_and_its_reach() {
        let mut v = viewer();
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        for aspect in [16.0 / 9.0, 1.0, 0.5] {
            cam.aspect = aspect;
            for (reach, height) in [(0.5, 1.8), (2.4, 2.2), (1.0, 6.0)] {
                v.orbit = Orbit::START;
                v.aim(&mut cam, reach, height);
                for yaw in [0.0, 0.7, 2.1, 4.0] {
                    cam.yaw = yaw;
                    for a in [0.0_f32, 1.5, 3.0, 4.5] {
                        for y in [0.0, height] {
                            let p = Vec3::new(
                                v.spot.x + reach * a.sin(),
                                v.ground + y,
                                v.spot.y + reach * a.cos(),
                            );
                            // A corner on the cylinder is inside the window
                            // from the default view, whichever way it turns.
                            let (sx, sy) = on_screen(&cam, p);
                            assert!(sx.abs() <= 1.0 && sy.abs() <= 1.0, "{sx} {sy}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn dragging_turns_tilts_and_stops_at_the_limits() {
        let mut v = viewer();
        v.press(0, false, (100.0, 100.0));
        v.drag_to((200.0, 100.0), (1000, 500));
        // A tenth of the window right: a tenth of a turn, the model's front
        // coming round to the right (the camera goes left, so yaw falls).
        assert!(
            (v.orbit.yaw + 0.1 * TURN_PER_WINDOW).abs() < 1e-4,
            "{}",
            v.orbit.yaw
        );
        v.drag_to((200.0, 5000.0), (1000, 500));
        assert_eq!(v.orbit.pitch, MAX_PITCH);
        v.drag_to((200.0, -90000.0), (1000, 500));
        assert_eq!(v.orbit.pitch, MIN_PITCH);
        v.release(0);
        let before = v.orbit;
        v.drag_to((0.0, 0.0), (1000, 500));
        assert_eq!(v.orbit, before, "no drag, no change");
        // Turning all the way round is allowed.
        v.turn(10.0);
        assert!(v.orbit.yaw.abs() > std::f32::consts::TAU - 0.5);
        v.reset_view();
        assert_eq!(v.orbit, Orbit::START);
    }

    #[test]
    fn zoom_is_limited_and_moving_is_kept_near_the_model() {
        let mut v = viewer();
        for _ in 0..100 {
            v.zoom(0.5);
        }
        assert_eq!(v.orbit.zoom, ZOOM_RANGE.0);
        for _ in 0..100 {
            v.zoom(2.0);
        }
        assert_eq!(v.orbit.zoom, ZOOM_RANGE.1);
        v.zoom(f32::NAN);
        v.zoom(0.0);
        assert_eq!(v.orbit.zoom, ZOOM_RANGE.1);
        v.press(1, false, (0.0, 0.0));
        v.drag_to((-100000.0, 100000.0), (1000, 500));
        let reach = v.look().map_or(1.0, |l| l.reach.max(l.height));
        assert!(v.orbit.shift.x.abs() <= reach * 1.5 + 1e-3);
        assert!(v.orbit.shift.y.abs() <= reach * 1.5 + 1e-3);
        // Shift + left moves as the right button does.
        v.release(1);
        v.press(0, true, (0.0, 0.0));
        assert_eq!(v.drag.map(|d| d.0), Some(Drag::Move));
        v.release(0);
        v.press(0, false, (0.0, 0.0));
        assert_eq!(v.drag.map(|d| d.0), Some(Drag::Turn));
        // Another button coming up does not end the drag.
        v.release(1);
        assert!(v.drag.is_some());
        v.release(0);
        assert!(v.drag.is_none());
    }

    #[test]
    fn a_camera_never_comes_closer_than_the_nearest_distance_and_looks_at_the_model() {
        let mut v = viewer();
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        v.orbit.zoom = ZOOM_RANGE.0;
        v.update(0.016, &mut cam);
        assert!(cam.distance >= NEAREST);
        assert!((cam.focus - v.spot).length() < 1e-3);
        assert!(cam.focus_height > v.ground);
        // The centre of the window is on the line to the model.
        let (x, y) = on_screen(&cam, Vec3::new(cam.focus.x, cam.focus_height, cam.focus.y));
        assert!(x.abs() < 1e-3 && y.abs() < 1e-3);
        assert_eq!(cam.pitch, START_PITCH);
    }

    #[test]
    fn the_animation_restarts_for_a_new_unit_or_clip() {
        let mut v = viewer();
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        v.update(1.0, &mut cam);
        v.update(1.0, &mut cam);
        assert!((v.time - 2.0).abs() < 1e-4);
        v.panel.step(1);
        v.update(0.5, &mut cam);
        assert_eq!(v.time, 0.0);
        v.update(0.5, &mut cam);
        assert!((v.time - 0.5).abs() < 1e-4);
    }
}
