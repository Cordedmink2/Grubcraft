//! The director camera (docs/30-cinematic-lighting.md, stage 4): a camera
//! that finds the fight and frames it, for watching a replay as a film or an
//! AI battle without touching the keys. Presentation only: it moves the
//! bird's-eye camera and, in a watched replay, slows the playback for the
//! big moments. It never touches the simulation; a live match's speed
//! cannot change (every machine must run it at the same pace), so slow
//! motion is for replays alone (`Director::pace`).
//!
//! What it watches is "heat": blows, ambushes, falling buildings and
//! routs leave a spot of heat that cools over a few seconds, and regiments
//! in melee are a steady warmth. Each frame it frames the warmest cluster,
//! easing the camera over rather than cutting, and only moves on when
//! another fight is clearly hotter and it has stayed a few seconds.

use glam::Vec2;

use crate::camera::{MAX_DISTANCE, MIN_DISTANCE, RtsCamera};
use crate::land::smooth;

/// How long a spot of heat takes to cool by a factor of e, seconds.
const HEAT_LIFE: f32 = 6.0;
/// Heat below this is dropped.
const COLD: f32 = 0.05;
/// A cluster is the heat within this many tiles.
const CLUSTER: f32 = 24.0;
/// The warmth of a regiment in melee, and of any regiment at all.
const ENGAGED: f32 = 1.0;
const PRESENT: f32 = 0.3;
/// How often the regiments in melee are looked at again, seconds.
const SCENE_EVERY: f32 = 0.4;
/// Stay with a fight at least this long, seconds, and leave it for one this
/// many times hotter.
const DWELL: f32 = 3.5;
const HOTTER: f32 = 1.35;
/// How fast the camera eases to what it frames, 1/seconds.
const FOLLOW: f32 = 0.9;
/// The closest and widest the director frames, tiles from the ground, and
/// how much room it leaves round a cluster.
const CLOSEST: f32 = 34.0;
const WIDEST: f32 = 120.0;
const ROOM: f32 = 2.2;
const ROOM_FLOOR: f32 = 26.0;
/// How far, in radians, and how slowly, the view drifts round the action.
const DRIFT: f32 = 0.3;
const DRIFT_RATE: f32 = 0.07;
/// A blow at least this hot slows a watched replay, to this pace, for this
/// long (real seconds), then not again for the cooldown.
pub const BIG: f32 = 3.5;
const SLOW_PACE: f32 = 0.35;
const SLOW_TIME: f32 = 1.6;
const SLOW_COOLDOWN: f32 = 8.0;
/// How far the camera may differ from where the director put it before the
/// player is taken to have moved it: only rounding, since nothing else
/// moves the camera between two frames.
const MOVED: f32 = 1e-4;
/// A blow within this many tiles of a spot of heat joins it, so a long fight
/// is one spot and not thousands, and the heat never grows past the cap.
const MERGE: f32 = 4.0;
const MOST_HEAT: f32 = 12.0;
const MAX_SPOTS: usize = 128;

/// How hot each kind of moment is.
pub const CHARGE_HIT: f32 = 3.6;
pub const AMBUSH: f32 = 4.0;
pub const BUILDING_FELL: f32 = 4.0;
pub const ROUT: f32 = 2.0;
/// Heat for each soldier lost.
pub const PER_LOSS: f32 = 0.1;

/// A spot of heat.
#[derive(Clone, Copy)]
struct Hot {
    at: Vec2,
    heat: f32,
}

#[derive(Default)]
pub struct Director {
    on: bool,
    heat: Vec<Hot>,
    /// Where the regiments in melee are, as of the last look, and where
    /// every regiment is (a calm moment frames the biggest group).
    melee: Vec<Vec2>,
    everyone: Vec<Vec2>,
    since_scene: f32,
    /// What is being framed: the middle, and how far from the ground.
    goal: Vec2,
    distance: f32,
    since_switch: f32,
    clock: f32,
    /// The camera as the director left it, to see whether the player moved it.
    left: Option<(Vec2, f32, f32)>,
    /// Real seconds left of slow motion, and until the next one.
    slow_left: f32,
    cooldown: f32,
}

impl Director {
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Take the camera over from where it is now.
    pub fn start(&mut self, camera: &RtsCamera) {
        *self = Director {
            on: true,
            goal: camera.focus,
            distance: camera.distance,
            since_switch: DWELL,
            // Look at the regiments at once.
            since_scene: SCENE_EVERY,
            ..Director::default()
        };
    }

    /// Hand the camera back.
    pub fn stop(&mut self) {
        *self = Director::default();
    }

    /// Something happened at `at`, `heat` hot (see [`CHARGE_HIT`]). A big
    /// one, when nothing else has just slowed the playback, starts slow
    /// motion.
    pub fn note(&mut self, at: Vec2, heat: f32) {
        if !self.on || heat.is_nan() || heat <= 0.0 {
            return;
        }
        let near = (self.heat.iter()).position(|h| h.at.distance_squared(at) <= MERGE * MERGE);
        if let Some(i) = near {
            self.heat[i].heat = (self.heat[i].heat + heat).min(MOST_HEAT);
        } else if self.heat.len() < MAX_SPOTS {
            self.heat.push(Hot { at, heat });
        } else if let Some(coolest) =
            (self.heat.iter_mut()).min_by(|a, b| a.heat.total_cmp(&b.heat))
            && coolest.heat < heat
        {
            // Full: only a blow hotter than the coolest spot is worth a place.
            *coolest = Hot { at, heat };
        }
        if heat >= BIG && self.slow_left <= 0.0 && self.cooldown <= 0.0 {
            self.slow_left = SLOW_TIME;
            self.cooldown = SLOW_COOLDOWN;
        }
    }

    /// How fast a watched replay should play for now: 1 normally, less in
    /// the slow motion of a big moment. A live match must not use this.
    pub fn pace(&self) -> f32 {
        if self.slow_left <= 0.0 {
            return 1.0;
        }
        // Eases down, holds, eases back up.
        let t = 1.0 - self.slow_left / SLOW_TIME;
        let ease = smooth(0.0, 0.25, t) - smooth(0.7, 1.0, t);
        1.0 - (1.0 - SLOW_PACE) * ease
    }

    /// Frame the action for a frame `dt` seconds long. `look` is called now
    /// and then for where the regiments in melee are and where every
    /// regiment is (only those the viewer sees). Turns itself off, leaving the camera
    /// as it is, when the player has moved the camera.
    pub fn step(
        &mut self,
        dt: f32,
        camera: &mut RtsCamera,
        look: impl FnOnce() -> (Vec<Vec2>, Vec<Vec2>),
    ) {
        if !self.on {
            return;
        }
        if let Some((focus, distance, yaw)) = self.left
            && (focus.distance(camera.focus) > MOVED
                || (distance - camera.distance).abs() > MOVED
                || (yaw - camera.yaw).abs() > MOVED)
        {
            self.stop();
            return;
        }
        self.clock += dt;
        self.since_switch += dt;
        self.since_scene += dt;
        self.slow_left = (self.slow_left - dt).max(0.0);
        self.cooldown = (self.cooldown - dt).max(0.0);
        let cool = (-dt / HEAT_LIFE).exp();
        self.heat.iter_mut().for_each(|h| h.heat *= cool);
        self.heat.retain(|h| h.heat > COLD);
        // Choosing between fights is the costly part: only now and then.
        let choose = self.since_scene >= SCENE_EVERY;
        if choose {
            self.since_scene = 0.0;
            (self.melee, self.everyone) = look();
        }
        self.frame_the_hottest(choose);
        let k = 1.0 - (-dt * FOLLOW).exp();
        camera.focus += (self.goal - camera.focus) * k;
        camera.focus = camera.focus.clamp(Vec2::ZERO, Vec2::splat(camera.map_size));
        camera.set_distance(camera.distance + (self.distance - camera.distance) * k);
        let yaw = camera.home_yaw + DRIFT * (self.clock * DRIFT_RATE * std::f32::consts::TAU).sin();
        camera.yaw += (yaw - camera.yaw) * k;
        self.left = Some((camera.focus, camera.distance, camera.yaw));
    }

    /// Frame the cluster the goal is in, and when `choose`, first move to
    /// another that is clearly hotter if the dwell is over.
    fn frame_the_hottest(&mut self, choose: bool) {
        // Candidates are the fights; with none, wherever regiments stand.
        let fights = || {
            (self.heat.iter().map(|h| (h.at, h.heat)))
                .chain(self.melee.iter().map(|&at| (at, ENGAGED)))
        };
        let points = || fights().chain(self.everyone.iter().map(|&at| (at, PRESENT)));
        let warmth = |at: Vec2| {
            (points().filter(|(p, _)| p.distance_squared(at) <= CLUSTER * CLUSTER))
                .map(|(_, w)| w)
                .sum::<f32>()
        };
        let mut centre = self.goal;
        if choose && self.since_switch >= DWELL {
            let candidates: Vec<Vec2> = match fights().next() {
                Some(_) => fights().map(|(at, _)| at).collect(),
                None => self.everyone.clone(),
            };
            let best = (candidates.into_iter())
                .map(|at| (at, warmth(at)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((at, hot)) = best
                && (hot > warmth(self.goal) * HOTTER || warmth(self.goal) <= 0.0)
            {
                self.since_switch = 0.0;
                centre = at;
            }
        }
        // Frame the weighted middle of the cluster, and how far it spreads.
        let (mut sum, mut weight, mut seen) = (Vec2::ZERO, 0.0, Vec::new());
        for (at, w) in points().filter(|(p, _)| p.distance_squared(centre) <= CLUSTER * CLUSTER) {
            sum += at * w;
            weight += w;
            seen.push(at);
        }
        if weight <= 0.0 {
            // Nobody in sight: hold the view, wide.
            self.distance = WIDEST;
            return;
        }
        self.goal = sum / weight;
        let spread = seen
            .iter()
            .map(|p| p.distance(self.goal))
            .fold(0.0, f32::max);
        self.distance = (spread * ROOM + ROOM_FLOOR).clamp(CLOSEST, WIDEST);
    }
}

const _: () = assert!(CLOSEST >= MIN_DISTANCE && WIDEST <= MAX_DISTANCE);

#[cfg(test)]
mod tests {
    use super::*;

    fn camera_at(focus: Vec2) -> RtsCamera {
        RtsCamera::new(768.0, focus)
    }

    /// Every regiment but the ones in `melee` stands at (200, 200).
    fn run(d: &mut Director, cam: &mut RtsCamera, seconds: f32, melee: &[Vec2]) {
        let mut t = 0.0;
        while t < seconds {
            d.step(0.1, cam, || {
                let all = melee.iter().copied().chain([Vec2::new(200.0, 200.0)]);
                (melee.to_vec(), all.collect())
            });
            t += 0.1;
        }
    }

    #[test]
    fn it_finds_a_fight_and_closes_in() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        let fight = [Vec2::new(300.0, 250.0), Vec2::new(306.0, 252.0)];
        run(&mut d, &mut cam, 20.0, &fight);
        assert!(d.is_on());
        assert!(cam.focus.distance(Vec2::new(303.0, 251.0)) < 10.0);
        assert!(cam.distance < 80.0, "distance {}", cam.distance);
    }

    #[test]
    fn a_calm_field_is_framed_on_the_biggest_group() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        run(&mut d, &mut cam, 30.0, &[]);
        assert!(cam.focus.distance(Vec2::new(200.0, 200.0)) < 10.0);
    }

    #[test]
    fn it_stays_with_a_fight_until_another_is_clearly_hotter() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        let near = [Vec2::new(150.0, 150.0), Vec2::new(152.0, 150.0)];
        run(&mut d, &mut cam, 10.0, &near);
        // One more regiment far off is not clearly hotter than two.
        d.note(Vec2::new(500.0, 500.0), 1.0);
        run(&mut d, &mut cam, 5.0, &near);
        assert!(cam.focus.distance(Vec2::new(151.0, 150.0)) < 15.0);
        // A big blow far off is.
        d.note(Vec2::new(500.0, 500.0), AMBUSH * 3.0);
        run(&mut d, &mut cam, 6.0, &near);
        assert!(cam.focus.distance(Vec2::new(500.0, 500.0)) < 100.0);
        // It cools, and the camera goes back to the melee.
        run(&mut d, &mut cam, 40.0, &near);
        assert!(cam.focus.distance(Vec2::new(151.0, 150.0)) < 30.0);
    }

    #[test]
    fn moving_the_camera_takes_it_back() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        run(&mut d, &mut cam, 2.0, &[Vec2::new(150.0, 150.0)]);
        assert!(d.is_on());
        cam.pan(0.05, 0.0);
        let moved = cam.focus;
        run(&mut d, &mut cam, 1.0, &[Vec2::new(150.0, 150.0)]);
        assert!(!d.is_on());
        assert_eq!(cam.focus, moved);
    }

    #[test]
    fn a_big_blow_slows_playback_then_lets_go_and_waits_to_do_it_again() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        assert_eq!(d.pace(), 1.0);
        d.note(Vec2::new(100.0, 100.0), AMBUSH);
        let mut slowest: f32 = 1.0;
        for _ in 0..30 {
            d.step(0.1, &mut cam, || (vec![], vec![]));
            slowest = slowest.min(d.pace());
        }
        assert!(slowest <= SLOW_PACE + 0.05, "slowest {slowest}");
        assert_eq!(d.pace(), 1.0);
        // Another at once is not slowed (the cooldown).
        d.note(Vec2::new(100.0, 100.0), AMBUSH);
        assert_eq!(d.pace(), 1.0);
    }

    #[test]
    fn small_blows_and_a_director_that_is_off_do_not_slow_anything() {
        let mut d = Director::default();
        d.note(Vec2::ZERO, AMBUSH);
        assert_eq!(d.pace(), 1.0);
        let cam = camera_at(Vec2::ZERO);
        d.start(&cam);
        d.note(Vec2::ZERO, 1.0);
        assert_eq!(d.pace(), 1.0);
    }

    #[test]
    fn a_slow_turn_of_the_view_still_takes_it_back() {
        let mut cam = camera_at(Vec2::new(100.0, 100.0));
        let mut d = Director::default();
        d.start(&cam);
        run(&mut d, &mut cam, 2.0, &[Vec2::new(150.0, 150.0)]);
        // One frame of a held turn key at 144 fps.
        cam.rotate(0.007);
        run(&mut d, &mut cam, 0.1, &[Vec2::new(150.0, 150.0)]);
        assert!(!d.is_on());
    }

    #[test]
    fn a_long_fight_is_one_spot_of_heat_and_the_list_has_a_cap() {
        let cam = camera_at(Vec2::ZERO);
        let mut d = Director::default();
        d.start(&cam);
        for i in 0..5000 {
            d.note(Vec2::new(100.0 + (i % 3) as f32, 100.0), 0.2);
        }
        assert_eq!(d.heat.len(), 1);
        assert!(d.heat[0].heat <= MOST_HEAT);
        for i in 0..5000 {
            d.note(
                Vec2::new(10.0 * i as f32, 10.0 * i as f32),
                0.5 + (i % 7) as f32,
            );
        }
        assert!(d.heat.len() <= MAX_SPOTS);
    }

    #[test]
    fn a_charge_hitting_home_is_a_big_moment() {
        let cam = camera_at(Vec2::ZERO);
        let mut d = Director::default();
        d.start(&cam);
        d.note(Vec2::ZERO, CHARGE_HIT);
        assert!(d.slow_left > 0.0);
    }
}
