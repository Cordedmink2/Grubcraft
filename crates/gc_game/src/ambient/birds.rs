//! Wildlife, stage 4b (docs/34-ambient-life.md): flocks of doves feed on the
//! open ground near the camera and burst into the air when soldiers the
//! viewer sees come near. They fly off, are gone for a while, and come back
//! in to land when the soldiers have gone. A picture only: not in the
//! simulation, not saved, not sent, in nobody's way.
//!
//! Flocks belong to the same 20 by 20 tile cells as deer herds (a hash of the
//! cell, with its own salts, says whether there is a flock and where it
//! feeds), so a place keeps its flocks, and only cells near the camera hold
//! any.

use std::collections::{HashMap, HashSet};

use gc_map::Map;
use glam::Vec2;

use super::wildlife::{CELL, hash, open, reach, suits_at, turn_toward};

/// The share of cells with a flock where the land suits one.
const CHANCE: f32 = 0.3;
/// Doves in a flock, the least and how many more at most.
const SIZE: (u32, u32) = (4, 5);
/// How far a flock keeps from the camera's focus: as `wildlife::reach`, and
/// forgotten this much farther.
const KEEP: f32 = 12.0;
/// A flock takes off when a soldier is this close to it (in tiles), and may
/// land again when none is within `SAFE` of its feeding ground.
const STARTLE: f32 = 13.0;
const SAFE: f32 = 28.0;
/// How far from its middle a feeding bird strays, and how fast it struts.
const SCATTER: f32 = 3.5;
const STRUT: f32 = 0.7;
/// Seconds a bird stands or struts before it does the other.
const STAND: (f32, f32) = (1.5, 5.0);
const STRUT_FOR: (f32, f32) = (1.0, 3.0);
/// Flight: speed in tiles a second, how fast a bird climbs and sinks, the
/// height it cruises at (in tiles), and how quickly it turns (radians a
/// second).
const FLY: (f32, f32) = (6.0, 8.5);
const CLIMB: f32 = 3.2;
const SINK: f32 = 4.0;
const CRUISE: (f32, f32) = (5.0, 9.0);
const TURN: f32 = 3.0;
/// A fleeing flock is gone once every bird is this far (tiles) beyond the
/// camera's reach (and has flown for `FLEE_LEAST` seconds), or after
/// `FLEE_MOST` seconds, and stays gone for `GONE`
/// seconds (the least, and how much more at most). A returning flock starts
/// this far beyond the reach, so neither shows from the field.
const OUT_OF_SIGHT: f32 = 8.0;
const FLEE_MOST: f32 = 20.0;
const FLEE_LEAST: f32 = 3.0;
const GONE: (f32, f32) = (20.0, 35.0);
/// How quickly a flap plays compared with the clip.
pub const FLAP_RATE: f32 = 1.7;

/// What a bird is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Doing {
    Stand,
    Strut,
    /// In the air, flapping.
    Fly,
}

#[derive(Clone, Debug)]
struct Bird {
    pos: Vec2,
    /// How high the bird flies above the ground, in tiles.
    alt: f32,
    facing: Vec2,
    doing: Doing,
    /// Seconds left in a stand or strut; while flying, the height to cruise at.
    left: f32,
    goal: Vec2,
    speed: f32,
    phase: f32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mood {
    /// On the ground.
    Feeding,
    /// In the air, going away; `left` seconds until out of sight.
    Fleeing { left: f32 },
    /// Out of sight for `left` seconds.
    Gone { left: f32 },
    /// Flying in to land.
    Landing,
}

#[derive(Clone, Debug)]
struct Flock {
    ground: Vec2,
    birds: Vec<Bird>,
    mood: Mood,
    rng: u32,
}

/// A bird as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub pos: Vec2,
    /// Tiles above the ground.
    pub alt: f32,
    pub facing: Vec2,
    pub doing: Doing,
    pub phase: f32,
    pub scale: f32,
}

#[derive(Default)]
pub struct Birds {
    flocks: HashMap<(i32, i32), Flock>,
    /// Cells looked at that hold none, so they are not looked at every frame.
    empty: HashSet<(i32, i32)>,
    seen: Vec<Seen>,
}

impl Flock {
    fn unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn between(&mut self, range: (f32, f32)) -> f32 {
        range.0 + self.unit() * (range.1 - range.0)
    }

    fn near(&mut self, at: Vec2, reach: f32) -> Vec2 {
        let a = self.unit() * std::f32::consts::TAU;
        at + Vec2::new(a.cos(), a.sin()) * self.unit().sqrt() * reach
    }

    fn centre(&self) -> Vec2 {
        self.birds.iter().map(|b| b.pos).sum::<Vec2>() / self.birds.len().max(1) as f32
    }
}

impl Birds {
    /// Forget every flock (a new match).
    pub fn clear(&mut self) {
        self.flocks.clear();
        self.empty.clear();
        self.seen.clear();
    }

    /// Draw no birds (the game is behind a menu, or the view is not the
    /// field's), keeping the flocks as they are.
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The birds to draw, as of the last `advance`.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Move the flocks on by `dt` seconds of effects time, near `focus` with
    /// the camera `distance` away. `clear(p)` says a bird may stand at `p`,
    /// `tame(p)` that a building is near, and `threats` are the soldiers the
    /// viewer sees.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        map: &Map,
        clear: impl Fn(Vec2) -> bool,
        tame: impl Fn(Vec2) -> bool,
        threats: &[Vec2],
    ) {
        let dt = dt.min(0.25);
        let reach = reach(distance);
        let cells = |r: f32| {
            let lo = ((focus - Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            let hi = ((focus + Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            (lo.x..=hi.x).flat_map(move |x| (lo.y..=hi.y).map(move |y| (x, y)))
        };
        let centre_of = |c: (i32, i32)| (Vec2::new(c.0 as f32, c.1 as f32) + 0.5) * CELL as f32;
        let keep = reach + KEEP + CELL as f32;
        self.flocks
            .retain(|&c, _| centre_of(c).distance(focus) <= keep);
        self.empty.retain(|&c| centre_of(c).distance(focus) <= keep);
        for c in cells(reach) {
            if centre_of(c).distance(focus) > reach
                || self.flocks.contains_key(&c)
                || self.empty.contains(&c)
            {
                continue;
            }
            match found(c, map, &clear, &tame) {
                Some(mut f) => {
                    // Made with soldiers by it: it is not on the ground to be
                    // put up the moment it appears.
                    if threats.iter().any(|t| t.distance(f.ground) < SAFE) {
                        f.mood = Mood::Gone { left: 2.0 };
                    }
                    self.flocks.insert(c, f);
                }
                None => {
                    self.empty.insert(c);
                }
            }
        }
        if dt > 0.0 {
            for f in self.flocks.values_mut() {
                step(f, dt, map, &clear, threats, (focus, reach));
            }
        }
        self.seen.clear();
        for f in self.flocks.values() {
            if matches!(f.mood, Mood::Gone { .. }) {
                continue;
            }
            self.seen.extend(f.birds.iter().map(|b| Seen {
                pos: b.pos,
                alt: b.alt,
                facing: b.facing,
                doing: b.doing,
                phase: b.phase,
                scale: b.scale,
            }));
        }
    }
}

/// The flock of cell `c`, if it has one.
fn found(
    c: (i32, i32),
    map: &Map,
    clear: &impl Fn(Vec2) -> bool,
    tame: &impl Fn(Vec2) -> bool,
) -> Option<Flock> {
    if hash(c, 21) >= CHANCE {
        return None;
    }
    let ground = Vec2::new(
        (c.0 as f32 + 0.15 + 0.7 * hash(c, 22)) * CELL as f32,
        (c.1 as f32 + 0.15 + 0.7 * hash(c, 23)) * CELL as f32,
    );
    let fits = |p: Vec2| suits_at(map, p) && open(map, p) && clear(p);
    if !fits(ground) || tame(ground) {
        return None;
    }
    let mut flock = Flock {
        ground,
        birds: Vec::new(),
        mood: Mood::Feeding,
        rng: (hash(c, 24) * 4_294_967_000.0) as u32 | 1,
    };
    let n = SIZE.0 + (hash(c, 25) * (SIZE.1 + 1) as f32) as u32;
    for k in 0..n {
        let pos = flock.near(ground, SCATTER);
        let pos = if fits(pos) { pos } else { ground };
        let a = flock.unit() * std::f32::consts::TAU;
        let doing = if flock.unit() < 0.5 {
            Doing::Stand
        } else {
            Doing::Strut
        };
        let left = flock.between(STAND) * flock.unit();
        let phase = k as f32 * 0.31 + flock.unit();
        let scale = 1.25 + 0.3 * flock.unit();
        flock.birds.push(Bird {
            pos,
            alt: 0.0,
            facing: Vec2::from_angle(a),
            doing,
            left,
            goal: pos,
            speed: 0.0,
            phase,
            scale,
        });
    }
    Some(flock)
}

/// Move one flock on by `dt`.
fn step(
    f: &mut Flock,
    dt: f32,
    map: &Map,
    clear: &impl Fn(Vec2) -> bool,
    threats: &[Vec2],
    view: (Vec2, f32),
) {
    let nearest = |at: Vec2| {
        threats
            .iter()
            .copied()
            .min_by(|a, b| a.distance_squared(at).total_cmp(&b.distance_squared(at)))
            .map(|t| (t, t.distance(at)))
    };
    match f.mood {
        Mood::Feeding => {
            if let Some((from, _)) = nearest(f.centre()).filter(|&(_, d)| d < STARTLE) {
                take_off(f, from);
                return;
            }
            for i in 0..f.birds.len() {
                let (spot, stand, strut, pick) = (
                    f.near(f.ground, SCATTER),
                    f.between(STAND),
                    f.between(STRUT_FOR),
                    f.unit(),
                );
                let b = &mut f.birds[i];
                match b.doing {
                    Doing::Strut => {
                        b.left -= dt;
                        let to = b.goal - b.pos;
                        if b.left <= 0.0 || to.length() < 0.2 {
                            b.doing = Doing::Stand;
                            b.left = stand;
                        } else {
                            let want = to.normalize();
                            b.facing = turn_toward(b.facing, want, 6.0 * dt);
                            if b.facing.dot(want) > 0.6 {
                                let next = b.pos + b.facing * STRUT * dt;
                                if open(map, next) && clear(next) {
                                    b.pos = next;
                                } else {
                                    b.left = 0.0;
                                }
                            }
                        }
                    }
                    _ => {
                        b.left -= dt;
                        if b.left <= 0.0 {
                            if pick < 0.5 && open(map, spot) && clear(spot) {
                                b.doing = Doing::Strut;
                                b.goal = spot;
                                b.left = strut;
                            } else {
                                b.doing = Doing::Stand;
                                b.left = stand;
                            }
                        }
                    }
                }
            }
        }
        Mood::Fleeing { left } => {
            // `goal` holds a fleeing bird's heading, a unit vector.
            for b in &mut f.birds {
                fly(b, b.goal, dt);
            }
            let left = left - dt;
            let (focus, reach) = view;
            let out = f
                .birds
                .iter()
                .all(|b| b.pos.distance(focus) > reach + OUT_OF_SIGHT);
            f.mood = if left <= 0.0 || (out && left < FLEE_MOST - FLEE_LEAST) {
                Mood::Gone {
                    left: f.between(GONE),
                }
            } else {
                Mood::Fleeing { left }
            };
        }
        Mood::Gone { left } => {
            let left = left - dt;
            let home_threat = nearest(f.ground).filter(|&(_, d)| d < SAFE);
            if left > 0.0 || home_threat.is_some() {
                f.mood = Mood::Gone {
                    left: if home_threat.is_some() {
                        left.max(2.0)
                    } else {
                        left
                    },
                };
                return;
            }
            // Come in from a random side, from beyond the camera's reach, high,
            // and sink as the ground nears.
            let side = f.unit() * std::f32::consts::TAU;
            let (focus, reach) = view;
            let from = focus + Vec2::from_angle(side) * (reach + OUT_OF_SIGHT);
            for i in 0..f.birds.len() {
                let (off, speed, alt) =
                    (f.near(Vec2::ZERO, 5.0), f.between(FLY), f.between(CRUISE));
                let landing = f.near(f.ground, SCATTER);
                let landing = if open(map, landing) && clear(landing) {
                    landing
                } else {
                    f.ground
                };
                let b = &mut f.birds[i];
                b.pos = from + off;
                b.alt = alt;
                b.speed = speed;
                b.doing = Doing::Fly;
                b.goal = landing;
                b.facing = (landing - b.pos).normalize_or(Vec2::X);
            }
            f.mood = Mood::Landing;
        }
        Mood::Landing => {
            let mut down = true;
            for b in &mut f.birds {
                if b.doing == Doing::Fly {
                    let to = b.goal - b.pos;
                    let far = to.length();
                    b.facing = turn_toward(b.facing, to.normalize_or(b.facing), TURN * dt);
                    b.pos += b.facing * b.speed.min(far / dt.max(1e-3)) * dt;
                    // Sink so the bird arrives low: its height follows its distance.
                    let want = (far * 0.45).min(9.0);
                    b.alt = if b.alt > want {
                        (b.alt - SINK * dt).max(want)
                    } else {
                        b.alt
                    };
                    if far < 0.3 && b.alt < 0.15 {
                        b.doing = Doing::Stand;
                        b.alt = 0.0;
                        b.pos = b.goal;
                        b.left = 1.0;
                    }
                }
                down &= b.doing != Doing::Fly;
            }
            // A soldier near a landing flock puts it up again.
            if !down && let Some((from, _)) = nearest(f.centre()).filter(|&(_, d)| d < STARTLE) {
                take_off(f, from);
                return;
            }
            if down {
                f.mood = Mood::Feeding;
            }
        }
    }
}

/// Put the flock into the air, every bird heading away from `from` on its
/// own line (those already flying keep their height).
fn take_off(f: &mut Flock, from: Vec2) {
    f.mood = Mood::Fleeing { left: FLEE_MOST };
    for i in 0..f.birds.len() {
        let (jitter, speed, cruise, lag) =
            (f.unit() - 0.5, f.between(FLY), f.between(CRUISE), f.unit());
        let b = &mut f.birds[i];
        let away = (b.pos - from).normalize_or(Vec2::X);
        if b.doing != Doing::Fly {
            // A few leap a moment late.
            b.alt = lag * 0.2;
        }
        b.doing = Doing::Fly;
        b.left = cruise;
        b.speed = speed;
        b.goal = Vec2::from_angle(jitter * 0.9).rotate(away);
    }
}

/// One bird in flight going `dir`: climbs to its cruising height (`left`)
/// and flies on.
fn fly(b: &mut Bird, dir: Vec2, dt: f32) {
    b.facing = turn_toward(b.facing, dir, TURN * dt);
    b.pos += b.facing * b.speed * dt;
    b.alt = (b.alt + CLIMB * dt).min(b.left);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_map::grown::Habitat;

    /// A small grown map whose every tile is `h`.
    fn land(h: Habitat) -> Map {
        let mut map = gc_map::FieldMap {
            choice: gc_map::GROWN + 1,
            tiles: 128,
            seed: 5,
        }
        .make();
        map.habitat.fill(h as u8);
        map
    }

    const FOCUS: Vec2 = Vec2::new(64.0, 64.0);

    fn run_for(b: &mut Birds, map: &Map, seconds: f32, threats: &[Vec2]) {
        for _ in 0..(seconds * 20.0) as usize {
            b.advance(0.05, FOCUS, 30.0, map, |_| true, |_| false, threats);
        }
    }

    fn some_flock(b: &Birds) -> (i32, i32) {
        *b.flocks
            .iter()
            .min_by(|a, b| {
                a.1.ground
                    .distance(FOCUS)
                    .total_cmp(&b.1.ground.distance(FOCUS))
            })
            .unwrap()
            .0
    }

    #[test]
    fn flocks_feed_where_the_land_suits_and_the_same_each_time() {
        let map = land(Habitat::Grassland);
        let (mut a, mut b) = (Birds::default(), Birds::default());
        run_for(&mut a, &map, 0.1, &[]);
        run_for(&mut b, &map, 0.1, &[]);
        assert!(!a.seen().is_empty());
        assert_eq!(a.seen().len(), b.seen().len());
        assert!(a.seen().iter().all(|s| s.alt == 0.0));
        let mut bog = Birds::default();
        run_for(&mut bog, &land(Habitat::PeatBog), 0.5, &[]);
        assert!(bog.seen().is_empty());
    }

    #[test]
    fn a_soldier_near_sends_the_flock_up_and_away_and_it_comes_back_to_land() {
        let map = land(Habitat::Grassland);
        let mut w = Birds::default();
        run_for(&mut w, &map, 1.0, &[]);
        let key = some_flock(&w);
        let ground = w.flocks[&key].ground;
        let threat = ground + Vec2::new(6.0, 0.0);
        run_for(&mut w, &map, 3.0, &[threat]);
        let flock = &w.flocks[&key];
        assert!(matches!(flock.mood, Mood::Fleeing { .. }));
        assert!(
            flock
                .birds
                .iter()
                .all(|b| b.doing == Doing::Fly && b.alt > 1.0)
        );
        assert!(flock.centre().distance(threat) > ground.distance(threat) + 5.0);
        // Out of sight a while (none of its birds drawn), then, the soldier
        // gone, back down to feed.
        let drawn = w.seen().len();
        run_for(&mut w, &map, 12.0, &[threat]);
        assert!(matches!(w.flocks[&key].mood, Mood::Gone { .. }));
        assert_eq!(w.seen().len(), drawn - w.flocks[&key].birds.len());
        run_for(&mut w, &map, 90.0, &[]);
        let flock = &w.flocks[&key];
        assert_eq!(flock.mood, Mood::Feeding);
        assert!(
            flock
                .birds
                .iter()
                .all(|b| b.alt == 0.0 && b.pos.distance(ground) < SCATTER + 1.0)
        );
    }

    #[test]
    fn a_flock_stays_away_while_a_soldier_waits_by_its_feeding_ground() {
        let map = land(Habitat::Grassland);
        let mut w = Birds::default();
        run_for(&mut w, &map, 0.5, &[]);
        let key = some_flock(&w);
        let threat = w.flocks[&key].ground + Vec2::new(9.0, 0.0);
        run_for(&mut w, &map, 90.0, &[threat]);
        assert!(matches!(w.flocks[&key].mood, Mood::Gone { .. }));
    }

    #[test]
    fn a_zoomed_out_camera_keeps_its_flocks() {
        let map = land(Habitat::Grassland);
        let mut w = Birds::default();
        let frame = |w: &mut Birds| w.advance(0.05, FOCUS, 110.0, &map, |_| true, |_| false, &[]);
        frame(&mut w);
        let keys: HashSet<_> = w.flocks.keys().copied().collect();
        assert!(!keys.is_empty());
        for _ in 0..200 {
            frame(&mut w);
            assert_eq!(w.flocks.keys().copied().collect::<HashSet<_>>(), keys);
        }
    }

    #[test]
    fn a_landing_flock_is_put_up_again_by_a_soldier_and_never_goes_underground() {
        let map = land(Habitat::Grassland);
        let mut w = Birds::default();
        run_for(&mut w, &map, 0.5, &[]);
        let key = some_flock(&w);
        let ground = w.flocks[&key].ground;
        w.flocks.get_mut(&key).unwrap().mood = Mood::Gone { left: 0.0 };
        let frame = |w: &mut Birds, threats: &[Vec2]| {
            w.advance(0.05, FOCUS, 30.0, &map, |_| true, |_| false, threats);
            for s in w.seen() {
                assert!(s.alt.is_finite() && s.alt >= 0.0, "{s:?}");
            }
        };
        for _ in 0..30 {
            frame(&mut w, &[]);
        }
        assert_eq!(w.flocks[&key].mood, Mood::Landing);
        // A soldier at the place the birds are over: they go up, not out.
        let over = w.flocks[&key].centre();
        frame(&mut w, &[over + Vec2::new(4.0, 0.0)]);
        assert!(matches!(w.flocks[&key].mood, Mood::Fleeing { .. }));
        assert!(w.seen().len() >= w.flocks[&key].birds.len());
        for _ in 0..2400 {
            frame(&mut w, &[]);
        }
        assert_eq!(w.flocks[&key].mood, Mood::Feeding);
        assert!(
            w.flocks[&key]
                .birds
                .iter()
                .all(|b| b.pos.distance(ground) < SCATTER + 1.0)
        );
    }

    #[test]
    fn a_flock_made_beside_soldiers_is_not_on_the_ground() {
        let map = land(Habitat::Grassland);
        let mut w = Birds::default();
        w.advance(0.05, FOCUS, 30.0, &map, |_| true, |_| false, &[]);
        let ground = w.flocks.values().next().unwrap().ground;
        let mut b = Birds::default();
        b.advance(0.05, FOCUS, 30.0, &map, |_| true, |_| false, &[ground]);
        assert!(
            b.flocks
                .values()
                .any(|f| f.ground == ground && matches!(f.mood, Mood::Gone { .. }))
        );
    }
}
