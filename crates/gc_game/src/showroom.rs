//! The showroom (`--showroom`): chosen soldier models standing in a row on
//! an empty field, seen from a fixed camera, for reviewing them. The layout
//! and the clip cycle are plain functions, tested below.

use glam::{Vec2, Vec3};

use crate::camera::{MIN_DISTANCE, RtsCamera};
use gc_assets::anim::Mask;
use gc_render::{POSE_LAYERS, PoseLayer};

use crate::models::{Action, Clips, Pose, pose_flags};
use crate::moves::MoveClip;

/// Models in a row before the next row starts behind it.
const ROW_LEN: usize = 8;
/// A model's slot in the row: at least this wide, in tiles, ...
const SLOT_MIN: f32 = 1.5;
/// ... or this many times its height.
const SLOT_PER_HEIGHT: f32 = 1.1;
/// The gap between rows, in heights of the tallest model in the front row.
const ROW_GAP: f32 = 1.5;
/// How far a model reaches in front of and behind its spot, for fitting
/// the view.
const HALF_DEPTH: f32 = 0.5;
/// The whole arrangement fills this much of the window at most.
const FILL: f32 = 1.0 / 1.1;
/// Closest the camera comes (a lone model fills the window from near), and
/// the furthest, short of Commander View.
const NEAREST: f32 = 4.0;
const FURTHEST: f32 = 150.0;
/// Seconds each clip plays in the default cycle.
const CLIP_SECONDS: f32 = 4.0;
/// The way the models face: towards the camera, which sits south of the
/// focus when the view is not turned.
pub(crate) const FACING: Vec2 = Vec2::Y;

const _: () = assert!(FURTHEST < crate::COMMANDER_OUT);
const _: () = assert!(NEAREST < MIN_DISTANCE);

/// What a model shows: one of the clips soldiers play, a fall, a march
/// (walking legs under an attacking upper body, to show the pose pass's
/// masked layers; a model still on bone rows only walks) or a look (idle,
/// looking round and up and down; a model on bone rows only idles) or a
/// half of a matched kill (a killer's, else a victim's; a model with none
/// idles). See docs/33.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    Play(Action),
    Die,
    March,
    Look,
    Kill,
}

impl Clip {
    /// The cycle's order: idle, walk, attack, then fall.
    const CYCLE: [Clip; 4] = [
        Clip::Play(Action::Idle),
        Clip::Play(Action::Walk),
        Clip::Play(Action::Attack),
        Clip::Die,
    ];

    pub fn parse(name: &str) -> Option<Clip> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "idle" => Clip::Play(Action::Idle),
            "walk" => Clip::Play(Action::Walk),
            "attack" => Clip::Play(Action::Attack),
            "shoot" => Clip::Play(Action::Shoot),
            "cheer" => Clip::Play(Action::Cheer),
            "die" => Clip::Die,
            "march" => Clip::March,
            "look" => Clip::Look,
            "kill" => Clip::Kill,
            _ => return None,
        })
    }
}

/// How long a kill's last frame holds before it plays again.
const KILL_HOLD_FRAMES: u32 = 18;

/// The clip playing `time` seconds in, and how many seconds into it: the
/// `hold` clip looping (a fall plays once and holds), or with none held,
/// every clip in turn, `CLIP_SECONDS` each, and round again.
fn at(hold: Option<Clip>, time: f32) -> (Clip, f32) {
    let time = time.max(0.0);
    if let Some(clip) = hold {
        return (clip, time);
    }
    let round = time % (CLIP_SECONDS * Clip::CYCLE.len() as f32);
    let k = ((round / CLIP_SECONDS) as usize).min(Clip::CYCLE.len() - 1);
    (Clip::CYCLE[k], round - k as f32 * CLIP_SECONDS)
}

/// What a model shows at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shown {
    pub pose: Pose,
    /// Lay it flat (a fall on a model with no death clip).
    pub flat: bool,
    pub layers: [PoseLayer; POSE_LAYERS],
    /// Where it looks, and how much ([`gc_render::SoldierInstance::look`]).
    pub look: [i8; 2],
    pub look_weight: u8,
    /// How much the feet keep to the move ([`gc_render::SoldierInstance::feet`]).
    pub feet: u8,
}

/// What `model` shows `time` seconds into the show.
pub(crate) fn pose(clips: &Clips, model: usize, hold: Option<Clip>, time: f32) -> Option<Shown> {
    let shown = |(pose, flat)| Shown {
        pose,
        flat,
        layers: Default::default(),
        look: [0; 2],
        look_weight: 0,
        feet: 0,
    };
    match at(hold, time) {
        (Clip::Die, t) => clips.death(model, t, None).map(|d| Shown {
            feet: crate::models::dying_feet(t),
            ..shown(d)
        }),
        (Clip::Play(action), t) => clips.pose(model, action, t).map(|p| shown((p, false))),
        (Clip::March, t) => {
            let mut s = shown((clips.pose(model, Action::Walk, t)?, false));
            if let Some(attack) = clips.clip(model, Action::Attack) {
                let upper = MoveClip {
                    mask: Mask::Upper,
                    ..attack
                };
                s.layers[0] = upper.layer(attack.looping(t), 1.0);
            }
            Some(s)
        }
        // Played through, held a moment, and again.
        (Clip::Kill, t) => match clips.kill_half(model) {
            Some(half) => {
                let frames = (half.frames + KILL_HOLD_FRAMES) as f32;
                Some(shown((
                    half.at_frame((t * half.fps).rem_euclid(frames)),
                    false,
                )))
            }
            None => clips
                .pose(model, Action::Idle, t)
                .map(|p| shown((p, false))),
        },
        (Clip::Look, t) => {
            let mut s = shown((clips.pose(model, Action::Idle, t)?, false));
            // Round to either side and back, nodding more quickly.
            let turn = |rate: f32, reach: f32| ((t * rate).sin() * reach).round() as i8;
            s.look = [turn(0.9, 127.0), turn(1.7, 90.0)];
            s.look_weight = 255;
            Some(s)
        }
    }
}

/// The models `spec` names: `all`, or names separated by commas, matched
/// without regard to case (blank entries are skipped). An unknown name, or
/// none at all, comes back in `Err`.
pub fn choose(spec: &str, models: &gc_assets::model::Models) -> Result<Vec<usize>, String> {
    // Soldiers are rigged; buildings stand still on the one still bone.
    let still = |m: &usize| models.models[*m].is_still();
    if spec.trim().eq_ignore_ascii_case("all") {
        return Ok((0..models.models.len()).filter(|m| !still(m)).collect());
    }
    if spec.trim().eq_ignore_ascii_case("buildings") {
        return Ok((0..models.models.len()).filter(still).collect());
    }
    let names: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return Err(String::new());
    }
    names
        .into_iter()
        .map(|n| {
            models
                .models
                .iter()
                .position(|m| m.name().eq_ignore_ascii_case(n))
                .ok_or_else(|| n.to_string())
        })
        .collect()
}

/// Where the models stand.
#[derive(Clone, Debug, PartialEq)]
struct Layout {
    /// Where each model stands, in the order asked for.
    positions: Vec<Vec2>,
    /// The arrangement's corners on the ground: slots across, and a model's
    /// depth front and back.
    low: Vec2,
    high: Vec2,
    /// Height of the tallest model.
    top: f32,
}

/// A model's slot across the row.
fn slot(height: f32) -> f32 {
    SLOT_MIN.max(SLOT_PER_HEIGHT * height)
}

/// Stand models of the given `heights` in rows of `ROW_LEN`, the first row
/// nearest the camera, centred on `centre`.
fn layout(heights: &[f32], centre: Vec2) -> Layout {
    let rows = heights.chunks(ROW_LEN);
    let gap = ROW_GAP
        * heights
            .iter()
            .take(ROW_LEN)
            .fold(0.0, |a: f32, &h| a.max(h));
    let mut positions = Vec::with_capacity(heights.len());
    let mut widest = 0.0_f32;
    let n_rows = rows.len();
    for (r, row) in rows.enumerate() {
        let width: f32 = row.iter().map(|&h| slot(h)).sum();
        widest = widest.max(width);
        let mut x = -width * 0.5;
        for &h in row {
            positions.push(Vec2::new(x + slot(h) * 0.5, -(r as f32) * gap));
            x += slot(h);
        }
    }
    // Rows run from z = 0 away from the camera; centre the lot on `centre`.
    let (front, back) = (HALF_DEPTH, -((n_rows.max(1) - 1) as f32) * gap - HALF_DEPTH);
    let shift = centre - Vec2::new(0.0, (front + back) * 0.5);
    for p in &mut positions {
        *p += shift;
    }
    Layout {
        positions,
        low: Vec2::new(-widest * 0.5, back) + shift,
        high: Vec2::new(widest * 0.5, front) + shift,
        top: heights.iter().fold(0.0, |a: f32, &h| a.max(h)),
    }
}

impl Layout {
    /// The ground point at the middle of the arrangement.
    fn focus(&self) -> Vec2 {
        (self.low + self.high) * 0.5
    }

    /// Hold `camera` on the arrangement, its models standing on ground
    /// between `low` and `high`, looking from the south at the usual pitch:
    /// at the middle height, from as near as lets every corner (lowest feet
    /// to the top of the tallest on the highest ground, slot edges, front to
    /// back) fit with a tenth to spare.
    fn aim(&self, camera: &mut RtsCamera, low: f32, high: f32) {
        let (bottom, top) = (low, high + self.top);
        camera.face(-FACING);
        camera.level_pitch();
        camera.focus = self.focus();
        camera.focus_height = (bottom + top) * 0.5;
        let corners = [self.low.x, self.high.x].into_iter().flat_map(|x| {
            [self.low.y, self.high.y]
                .into_iter()
                .flat_map(move |z| [bottom, top].map(|y| Vec3::new(x, y, z)))
        });
        camera.distance = camera.fit_distance(corners, FILL).clamp(NEAREST, FURTHEST);
    }
}

/// The models on show: which, where and doing what.
pub struct Showroom {
    models: Vec<usize>,
    layout: Layout,
    /// The one clip shown, or `None` to cycle through them.
    hold: Option<Clip>,
}

impl Showroom {
    /// Stand `models` (of these `heights`, by model) around `centre`.
    pub fn new(models: Vec<usize>, heights: &[f32], centre: Vec2, hold: Option<Clip>) -> Showroom {
        let tall: Vec<f32> = models.iter().map(|&m| heights[m]).collect();
        Showroom {
            layout: layout(&tall, centre),
            models,
            hold,
        }
    }

    /// Hold the camera on the models.
    pub fn aim(&self, camera: &mut RtsCamera, ground: impl Fn(Vec2) -> f32) {
        let (low, high) = self
            .layout
            .positions
            .iter()
            .map(|&p| ground(p))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), g| {
                (low.min(g), high.max(g))
            });
        self.layout.aim(camera, low, high);
    }

    /// Stand the models in this frame's drawing, `time` seconds in.
    pub fn push(
        &self,
        art: &mut crate::Art,
        ground: impl Fn(Vec2) -> f32,
        time: f32,
        color: [u8; 3],
    ) {
        for (&model, &pos) in self.models.iter().zip(&self.layout.positions) {
            // A building has only its still pose: it never walks or falls.
            let hold = if art.looks.is_still(model) {
                Some(Clip::Play(Action::Idle))
            } else {
                self.hold
            };
            if let Some(shown) = pose(&art.clips, model, hold, time) {
                stand(art, model, pos, ground(pos), 0.0, &shown, color);
            }
        }
    }
}

/// Stand `model` at `pos` on ground `ground` high, raised by `lift`, facing
/// the camera as `shown`, in this frame's drawing.
pub(crate) fn stand(
    art: &mut crate::Art,
    model: usize,
    pos: Vec2,
    ground: f32,
    lift: f32,
    shown: &Shown,
    color: [u8; 3],
) {
    let Shown {
        pose,
        flat,
        layers,
        look,
        look_weight,
        feet,
    } = *shown;
    let at = Vec3::new(pos.x, ground + lift, pos.y);
    let lod = art.view.lod(art.heights[model], at);
    art.buckets[model * gc_assets::model::LODS + lod].push(gc_render::SoldierInstance {
        pos: pos.to_array(),
        facing: FACING.to_array(),
        frames: pose.rows,
        flags: pose_flags(pose, 0, flat),
        color: [color[0], color[1], color[2], gc_render::TONE_NEUTRAL],
        lift,
        scale: 1.0,
        layers,
        look,
        look_weight,
        feet,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHTS: [f32; 5] = [2.1, 1.0, 0.8, 1.6, 2.4];

    /// A camera aimed at `l` on ground 10 high, in a window of `aspect`.
    fn aimed(l: &Layout, aspect: f32) -> RtsCamera {
        let mut cam = RtsCamera::new(256.0, Vec2::ZERO);
        cam.aspect = aspect;
        l.aim(&mut cam, 10.0, 10.0);
        cam
    }

    #[test]
    fn names_match_without_case_or_spaces() {
        let models = crate::models::load().unwrap();
        let (a, b) = (models.models[0].name(), models.models[1].name());
        // `all` is every soldier, `buildings` every building look.
        let soldiers: Vec<usize> = (0..models.models.len())
            .filter(|&m| !models.models[m].is_still())
            .collect();
        let buildings: Vec<usize> = (0..models.models.len())
            .filter(|&m| models.models[m].is_still())
            .collect();
        assert!(!soldiers.is_empty() && !buildings.is_empty());
        assert_eq!(soldiers.len() + buildings.len(), models.models.len());
        assert_eq!(choose("all", &models), Ok(soldiers.clone()));
        assert_eq!(choose(" ALL ", &models), Ok(soldiers));
        assert_eq!(choose("Buildings", &models), Ok(buildings));
        let both = format!(" {} , {}", b.to_lowercase(), a.to_uppercase());
        assert_eq!(choose(&both, &models), Ok(vec![1, 0]));
        assert_eq!(
            choose(&format!("{a},Spearman"), &models),
            Err("Spearman".into())
        );
        assert_eq!(choose("", &models), Err(String::new()));
        assert_eq!(choose(&format!("{a}, ,"), &models), Ok(vec![0]));
        for (k, m) in models.models.iter().enumerate() {
            assert_eq!(choose(&m.name().to_uppercase(), &models), Ok(vec![k]));
        }
    }

    #[test]
    fn slots_do_not_overlap() {
        let l = layout(&HEIGHTS, Vec2::new(100.0, 80.0));
        assert_eq!(l.positions.len(), HEIGHTS.len());
        for (a, b) in l.positions.iter().zip(l.positions.iter().skip(1)) {
            assert_eq!(a.y, b.y);
        }
        for i in 0..HEIGHTS.len() - 1 {
            let apart = l.positions[i + 1].x - l.positions[i].x;
            let wanted = (slot(HEIGHTS[i]) + slot(HEIGHTS[i + 1])) * 0.5;
            assert!((apart - wanted).abs() < 1e-4, "{i}: {apart} vs {wanted}");
        }
        // Small models still get the minimum, tall ones a bit more than
        // their height.
        assert_eq!(slot(0.8), SLOT_MIN);
        assert!((slot(2.4) - 2.64).abs() < 1e-5);
        // Centred on the middle asked for.
        let (first, last) = (l.positions[0].x, l.positions[4].x);
        let ends = (first - slot(HEIGHTS[0]) * 0.5 + last + slot(HEIGHTS[4]) * 0.5) * 0.5;
        assert!((ends - 100.0).abs() < 1e-3);
        assert!((l.focus() - Vec2::new(100.0, 80.0)).length() < 1e-4);
    }

    #[test]
    fn nine_models_wrap_to_a_second_row() {
        let heights = [2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.2];
        let l = layout(&heights, Vec2::new(50.0, 50.0));
        let front = l.positions[0].y;
        assert!(l.positions[..8].iter().all(|p| p.y == front));
        // The ninth stands behind (further from the camera, which is south:
        // smaller z) by 1.5 tallest in the front row, centred in its own row.
        let back = l.positions[8];
        assert!((front - back.y - ROW_GAP * 2.0).abs() < 1e-4);
        let row_two = layout(&[1.2], Vec2::new(50.0, 50.0));
        assert!((back.x - row_two.positions[0].x).abs() < 1e-4);
        // Sixteen make two full rows; seventeen make a third.
        let many = vec![1.0; 17];
        let l = layout(&many, Vec2::ZERO);
        let mut zs: Vec<f32> = l.positions.iter().map(|p| p.y).collect();
        zs.dedup();
        assert_eq!(zs.len(), 3);
        // The arrangement is centred front to back on the focus.
        let mid = (l.positions[0].y + l.positions[16].y) * 0.5;
        assert!(mid.abs() < 1e-4, "{mid}");
    }

    #[test]
    fn everything_fits_the_view_with_a_margin() {
        let cases: [(&[f32], f32); 5] = [
            (&HEIGHTS, 16.0 / 9.0),
            (&HEIGHTS, 1.0),
            (&HEIGHTS, 0.5),
            (&[1.0; 17], 16.0 / 9.0),
            (&[2.0; 17], 21.0 / 9.0),
        ];
        for (heights, aspect) in cases {
            let l = layout(heights, Vec2::new(128.0, 128.0));
            let cam = aimed(&l, aspect);
            assert!(cam.distance < crate::COMMANDER_IN);
            let m = cam.view_proj();
            let mut widest = 0.0_f32;
            for (p, &h) in l.positions.iter().zip(heights) {
                for (up, half) in [(0.0, slot(h) * 0.5), (h, slot(h) * 0.5)] {
                    for dx in [-half, half] {
                        let v = m * glam::Vec4::new(p.x + dx, 10.0 + up, p.y, 1.0);
                        let (x, y) = (v.x / v.w, v.y / v.w);
                        assert!(x.abs() <= FILL + 1e-3, "x {x} aspect {aspect}");
                        assert!(y.abs() <= FILL + 1e-3, "y {y} aspect {aspect}");
                        widest = widest.max(x.abs()).max(y.abs());
                    }
                }
            }
            // And the view is not wasted: something comes close to the margin.
            assert!(
                widest > 0.7,
                "{widest} for {} models at {aspect}",
                heights.len()
            );
        }
    }

    #[test]
    fn a_lone_model_is_seen_close_and_a_crowd_stays_out_of_commander_view() {
        let one = aimed(&layout(&[2.0], Vec2::ZERO), 16.0 / 9.0).distance;
        assert!((NEAREST..MIN_DISTANCE).contains(&one));
        let crowd = aimed(&layout(&[3.0; 200], Vec2::ZERO), 1.0).distance;
        assert!(crowd <= FURTHEST && FURTHEST < crate::COMMANDER_IN);
    }

    #[test]
    fn the_cycle_runs_idle_walk_attack_fall_and_round() {
        let [idle, walk, attack, die] = Clip::CYCLE;
        assert_eq!(at(None, 0.0), (idle, 0.0));
        assert_eq!(at(None, 3.5), (idle, 3.5));
        assert_eq!(at(None, 4.0), (walk, 0.0));
        assert_eq!(at(None, 8.5), (attack, 0.5));
        assert_eq!(at(None, 12.0), (die, 0.0));
        assert_eq!(at(None, 15.75), (die, 3.75));
        assert_eq!(at(None, 16.0), (idle, 0.0));
        assert_eq!(at(None, 36.5), (walk, 0.5));
        assert_eq!(at(None, -1.0), (idle, 0.0));
    }

    #[test]
    fn a_held_clip_keeps_going_and_a_fall_holds_its_last_frame() {
        let shoot = Clip::Play(Action::Shoot);
        assert_eq!(at(Some(shoot), 21.0), (shoot, 21.0));
        assert_eq!(Clip::parse(" Shoot "), Some(shoot));
        assert_eq!(Clip::parse("run"), None);

        let models = crate::models::load().unwrap();
        let clips = Clips::new(&models);
        let m = models.by_name("Bone Pikes").unwrap();
        let die = Some(Clip::Die);
        let start = pose(&clips, m, die, 0.0).unwrap().pose;
        let end = pose(&clips, m, die, 50.0).unwrap().pose;
        assert_ne!(start.rows, end.rows);
        assert_eq!(end, pose(&clips, m, die, 500.0).unwrap().pose);
        // A falling model's feet leave the ground's hold as he goes down.
        assert_eq!(pose(&clips, m, die, 0.0).unwrap().feet, 0);
        assert_eq!(
            pose(&clips, m, die, 50.0).unwrap().feet,
            gc_render::FEET_AS_POSED
        );
        assert_eq!(pose(&clips, m, None, 1.0).unwrap().feet, 0);
        // The cycle's fall starts where the clip does and ends held, too.
        assert_eq!(pose(&clips, m, None, 12.0).unwrap().pose, start);
        assert_eq!(pose(&clips, m, None, 15.9).unwrap().pose.rows, end.rows);
        // Every model has a pose in every clip.
        for m in 0..models.models.len() {
            for clip in [
                "idle", "walk", "attack", "shoot", "cheer", "die", "march", "look", "kill",
            ] {
                assert!(pose(&clips, m, Clip::parse(clip), 1.3).is_some());
            }
        }
        // A march swings the attack on the upper body over the walk.
        let march = pose(&clips, m, Some(Clip::March), 1.3).unwrap();
        assert_eq!(march.pose, clips.pose(m, Action::Walk, 1.3).unwrap());
        assert_eq!(march.layers[0].mask, Mask::Upper as u8);
        assert_eq!(march.layers[0].weight, 255);
        // A look idles, turning round.
        let look = pose(&clips, m, Some(Clip::Look), 1.3).unwrap();
        assert_eq!(look.pose, clips.pose(m, Action::Idle, 1.3).unwrap());
        assert_eq!(look.look_weight, 255);
        assert_ne!(
            look.look,
            pose(&clips, m, Some(Clip::Look), 2.6).unwrap().look
        );
    }
}
