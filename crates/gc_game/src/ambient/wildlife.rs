//! Wildlife, stage 4a (docs/34-ambient-life.md): small herds of deer graze
//! the open land near the camera, walk to new pasture now and then, and bolt
//! from soldiers the viewer sees. They are a picture: not in the simulation,
//! not saved, not sent, in nobody's way (soldiers walk through them), and
//! drawn only while the camera is near.
//!
//! A herd belongs to a 20 by 20 tile cell of the map, where a hash of the
//! cell says whether there is one and where its home is, so coming back to
//! a place finds the same herds in the same spots. Only the cells near the
//! camera hold herds; the rest are forgotten and made again, at home, when
//! the camera returns.

use std::collections::{HashMap, HashSet};

use gc_map::Map;
use gc_map::def::Land;
use gc_map::grown::Habitat;
use glam::Vec2;

/// A cell of the map, in tiles, and the share of cells with a herd where
/// the land suits one.
pub const CELL: i32 = 20;
const CHANCE: f32 = 0.4;
/// Deer in a herd, the least and how many more at most.
const SIZE: (u32, u32) = (2, 3);
/// How far from the camera's focus herds are kept: this at no distance, and
/// this much more per tile of camera distance, at most; cells farther than
/// this and `KEEP` more are forgotten.
const REACH_NEAR: f32 = 36.0;
const REACH_PER_ZOOM: f32 = 0.9;
const REACH_MOST: f32 = 75.0;
const KEEP: f32 = 12.0;
/// Speeds, in tiles a second, and how quickly a deer turns, in radians a
/// second.
const WALK: f32 = 0.9;
const RUN: f32 = 5.5;
const TURN_WALK: f32 = 2.5;
const TURN_RUN: f32 = 9.0;
/// How the clips are played to match the speed: a trot is a clip a second,
/// a bound is faster.
pub const RUN_RATE: f32 = 1.5;
/// A herd bolts from a soldier this close, runs until it is this far from
/// every one, and no soldier near for `CALM` seconds lets it settle.
const SPOOK: f32 = 15.0;
const SAFE: f32 = 30.0;
const CALM: f32 = 5.0;
/// The longest a bolt lasts, in seconds.
const BOLT_MOST: f32 = 9.0;
/// How far from home a herd roams, in tiles, and from its own pasture a
/// deer stands.
const ROAM: f32 = 9.0;
const SCATTER: f32 = 3.0;
/// How long a deer grazes or stands before it does something else, in
/// seconds: the least, and how much more at most.
const GRAZE: (f32, f32) = (5.0, 11.0);
const STAND: (f32, f32) = (1.5, 4.0);
/// How long before a herd moves to new pasture, in seconds.
const MOVE_EVERY: (f32, f32) = (25.0, 45.0);
/// Herds are not made within this many tiles of a building.
pub const TAME: f32 = 14.0;
/// The ground must be this far above the water; a step may climb this much
/// over `STEP` tiles.
const SHORE: f32 = 0.12;
const STEP: f32 = 0.4;
const STEEPEST: f32 = 0.3;

/// How far from the focus herds are made, in tiles, for a camera
/// `distance` away.
pub fn reach(distance: f32) -> f32 {
    (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST)
}

/// What a deer is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Doing {
    Graze,
    Stand,
    Walk,
    Bolt,
}

#[derive(Clone, Debug)]
struct Deer {
    pos: Vec2,
    facing: Vec2,
    doing: Doing,
    /// Seconds left in this doing (a walk ends on reaching `goal`, or when
    /// this runs out, so a deer that cannot get there gives up).
    left: f32,
    goal: Vec2,
    /// A walk just ordered, whose goal is not chosen yet.
    fresh: bool,
    /// Offsets the clip so a herd does not move as one, and the deer's size.
    phase: f32,
    scale: f32,
}

#[derive(Clone, Debug)]
struct Herd {
    home: Vec2,
    pasture: Vec2,
    deer: Vec<Deer>,
    rng: u32,
    /// Seconds until the herd moves to new pasture, and until a bolting
    /// herd stops (zero when it is not bolting).
    move_in: f32,
    bolt: f32,
    /// Seconds with no soldier near, while the herd is wary after a bolt.
    wary: f32,
}

/// A deer as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub pos: Vec2,
    pub facing: Vec2,
    pub doing: Doing,
    pub phase: f32,
    pub scale: f32,
}

#[derive(Default)]
pub struct Wildlife {
    herds: HashMap<(i32, i32), Herd>,
    /// Cells looked at that hold none (the land is wrong or a spot was
    /// not found), so they are not looked at again every frame.
    empty: HashSet<(i32, i32)>,
    seen: Vec<Seen>,
}

/// A number from 0 to 1 that stays the same for a cell and a `salt`.
fn hash(cell: (i32, i32), salt: u32) -> f32 {
    let mut x = (cell.0 as u32)
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add((cell.1 as u32).wrapping_mul(0x85eb_ca6b))
        .wrapping_add(salt.wrapping_mul(0xc2b2_ae35));
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1 << 24) as f32
}

/// Whether deer live on this habitat: open land and light woods, not bog,
/// rock or the waste.
fn suits(h: Habitat) -> bool {
    use Habitat::*;
    matches!(
        h,
        FlowerMeadow
            | Grassland
            | ScrubEdge
            | OldGrowth
            | BirchFringe
            | FernHollow
            | Glade
            | HeatherMoor
            | TreeLine
            | WetMeadow
    )
}

/// Whether deer live at `p`: by the habitat of a grown map, else by the
/// land of a painted one (a map with neither is open meadow).
fn suits_at(map: &Map, p: Vec2) -> bool {
    let tiles = map.tiles as i32;
    let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
    if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
        return false;
    }
    let tile = y as usize * tiles as usize + x as usize;
    if let Some(&b) = map.habitat.get(tile) {
        return suits(Habitat::from_u8(b));
    }
    map.land_at(tile)
        .is_none_or(|l| !matches!(l, Land::Fen | Land::Waste))
}

/// Whether a deer may stand at `p`: on the map, on dry ground.
fn open(map: &Map, p: Vec2) -> bool {
    let edge = 1.0;
    let side = map.tiles as f32 - edge;
    p.x > edge
        && p.y > edge
        && p.x < side
        && p.y < side
        && map.height_at(p.x, p.y) > map.water_level() + SHORE
}

/// Whether a deer may step from `a` to `b` (a little way): both open and
/// not too steep.
fn passable(map: &Map, a: Vec2, b: Vec2) -> bool {
    open(map, b) && (map.height_at(b.x, b.y) - map.height_at(a.x, a.y)).abs() <= STEEPEST
}

fn turn_toward(from: Vec2, to: Vec2, most: f32) -> Vec2 {
    if to.length_squared() < 1e-6 {
        return from;
    }
    let to = to.normalize();
    let angle = from.angle_to(to);
    Vec2::from_angle(angle.clamp(-most, most))
        .rotate(from)
        .normalize_or(to)
}

impl Herd {
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

    fn centre(&self) -> Vec2 {
        self.deer.iter().map(|d| d.pos).sum::<Vec2>() / self.deer.len().max(1) as f32
    }

    /// A random spot within `reach` of `at`.
    fn near(&mut self, at: Vec2, reach: f32) -> Vec2 {
        let a = self.unit() * std::f32::consts::TAU;
        at + Vec2::new(a.cos(), a.sin()) * self.unit().sqrt() * reach
    }
}

impl Wildlife {
    /// Forget every herd (a new match).
    pub fn clear(&mut self) {
        self.herds.clear();
        self.empty.clear();
        self.seen.clear();
    }

    /// Draw no deer (the game is behind a menu, or the view is not the
    /// field's), keeping the herds as they are.
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The deer to draw, as of the last `advance`.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Move the herds on by `dt` seconds of effects time, near `focus` with
    /// the camera `distance` away. `clear(p)` says a deer may stand at `p`
    /// to graze (no building or tree there), `tame(p)` that a building is
    /// near, and `threats` are the soldiers the viewer sees.
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
        let keep = reach + KEEP;
        let cells = |r: f32| {
            let lo = ((focus - Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            let hi = ((focus + Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            (lo.x..=hi.x).flat_map(move |x| (lo.y..=hi.y).map(move |y| (x, y)))
        };
        let centre_of = |c: (i32, i32)| (Vec2::new(c.0 as f32, c.1 as f32) + 0.5) * CELL as f32;
        self.herds
            .retain(|&c, _| centre_of(c).distance(focus) <= keep + CELL as f32);
        self.empty
            .retain(|&c| centre_of(c).distance(focus) <= keep + CELL as f32);
        for c in cells(reach) {
            // Made by the same measure `retain` keeps by, so a herd made
            // here is not dropped and made again in the next breath.
            if centre_of(c).distance(focus) > reach
                || self.herds.contains_key(&c)
                || self.empty.contains(&c)
            {
                continue;
            }
            match Self::found(c, map, &clear, &tame) {
                Some(h) => {
                    self.herds.insert(c, h);
                }
                None => {
                    self.empty.insert(c);
                }
            }
        }
        if dt > 0.0 {
            for h in self.herds.values_mut() {
                step(h, dt, map, &clear, threats);
            }
        }
        self.seen.clear();
        for h in self.herds.values() {
            for d in &h.deer {
                self.seen.push(Seen {
                    pos: d.pos,
                    facing: d.facing,
                    doing: d.doing,
                    phase: d.phase,
                    scale: d.scale,
                });
            }
        }
    }

    /// The herd of cell `c`, if it has one.
    fn found(
        c: (i32, i32),
        map: &Map,
        clear: &impl Fn(Vec2) -> bool,
        tame: &impl Fn(Vec2) -> bool,
    ) -> Option<Herd> {
        if hash(c, 1) >= CHANCE {
            return None;
        }
        let home = Vec2::new(
            (c.0 as f32 + 0.15 + 0.7 * hash(c, 2)) * CELL as f32,
            (c.1 as f32 + 0.15 + 0.7 * hash(c, 3)) * CELL as f32,
        );
        let fits = |p: Vec2| suits_at(map, p) && open(map, p) && clear(p);
        if !fits(home) || tame(home) {
            return None;
        }
        let mut herd = Herd {
            home,
            pasture: home,
            deer: Vec::new(),
            rng: (hash(c, 4) * 4_294_967_000.0) as u32 | 1,
            move_in: 0.0,
            bolt: 0.0,
            wary: 0.0,
        };
        herd.move_in = herd.between(MOVE_EVERY);
        let n = SIZE.0 + (hash(c, 5) * (SIZE.1 + 1) as f32) as u32;
        for k in 0..n {
            let pos = herd.near(home, SCATTER);
            let pos = if fits(pos) { pos } else { home };
            let a = herd.unit() * std::f32::consts::TAU;
            let doing = if herd.unit() < 0.7 {
                Doing::Graze
            } else {
                Doing::Stand
            };
            let left = herd.between(GRAZE) * herd.unit();
            let (phase, scale) = (k as f32 * 0.37 + herd.unit(), 0.9 + 0.2 * herd.unit());
            herd.deer.push(Deer {
                pos,
                facing: Vec2::from_angle(a),
                doing,
                left,
                goal: pos,
                fresh: false,
                phase,
                scale,
            });
        }
        Some(herd)
    }
}

/// Move one herd on by `dt`.
fn step(h: &mut Herd, dt: f32, map: &Map, clear: &impl Fn(Vec2) -> bool, threats: &[Vec2]) {
    let centre = h.centre();
    let nearest = threats.iter().copied().min_by(|a, b| {
        a.distance_squared(centre)
            .total_cmp(&b.distance_squared(centre))
    });
    let danger = nearest.map(|t| (t, t.distance(centre)));
    match danger {
        // A wary herd, standing alert, is startled again only by a soldier
        // well inside the spooking distance, so a herd that has bolted
        // for as long as it will does not start again at once.
        Some((_, d)) if h.bolt <= 0.0 && d < if h.wary > 0.0 { SPOOK * 0.5 } else { SPOOK } => {
            h.bolt = BOLT_MOST;
            h.wary = 0.0;
            for d in &mut h.deer {
                d.doing = Doing::Bolt;
            }
        }
        _ => {}
    }
    if h.bolt > 0.0 {
        let from = danger.map_or(centre, |(t, _)| t);
        h.bolt -= dt;
        let safe = danger.is_none_or(|(_, d)| d > SAFE);
        if safe || h.bolt <= 0.0 {
            h.bolt = 0.0;
            h.wary = CALM;
            for d in &mut h.deer {
                d.doing = Doing::Stand;
                d.left = CALM;
            }
        } else {
            for i in 0..h.deer.len() {
                let jitter = (h.unit() - 0.5) * 0.4;
                let d = &mut h.deer[i];
                let away = (d.pos - from).normalize_or(Vec2::X);
                run(
                    d,
                    Vec2::from_angle(jitter).rotate(away),
                    RUN,
                    TURN_RUN,
                    dt,
                    map,
                );
            }
            return;
        }
    }
    if h.wary > 0.0 {
        // Standing alert; a soldier coming back starts the bolt again, above,
        // and the herd stays wary while one is within `SAFE`.
        h.wary = if danger.is_some_and(|(_, d)| d < SAFE) {
            CALM
        } else {
            h.wary - dt
        };
        if h.wary <= 0.0 {
            h.pasture = h.home;
            for d in &mut h.deer {
                d.doing = Doing::Walk;
                d.fresh = true;
            }
        } else {
            return;
        }
    }
    h.move_in -= dt;
    if h.move_in <= 0.0 {
        h.move_in = h.between(MOVE_EVERY);
        for _ in 0..4 {
            let p = h.near(h.home, ROAM);
            if open(map, p) && clear(p) {
                h.pasture = p;
                break;
            }
        }
        for d in &mut h.deer {
            d.doing = Doing::Walk;
            d.fresh = true;
        }
    }
    for i in 0..h.deer.len() {
        let pick = h.unit();
        let spot = h.near(h.pasture, SCATTER);
        let rest = h.between(GRAZE);
        let stand = h.between(STAND);
        let d = &mut h.deer[i];
        match d.doing {
            Doing::Walk => {
                if d.fresh {
                    // Just told to walk: choose where in the pasture.
                    d.goal = if open(map, spot) && clear(spot) {
                        spot
                    } else {
                        h.pasture
                    };
                    d.left = d.goal.distance(d.pos) / WALK * 2.0 + 3.0;
                    d.fresh = false;
                }
                d.left -= dt;
                let to = d.goal - d.pos;
                if to.length() < 0.4 || d.left <= 0.0 {
                    d.doing = Doing::Graze;
                    d.left = rest;
                } else {
                    run(d, to, WALK, TURN_WALK, dt, map);
                    d.doing = Doing::Walk;
                }
            }
            Doing::Graze | Doing::Stand | Doing::Bolt => {
                d.left -= dt;
                if d.left <= 0.0 {
                    if d.doing == Doing::Graze && pick < 0.35 {
                        d.doing = Doing::Stand;
                        d.left = stand;
                    } else if pick < 0.5 {
                        d.doing = Doing::Walk;
                        d.fresh = true;
                    } else {
                        d.doing = Doing::Graze;
                        d.left = rest;
                    }
                }
            }
        }
    }
}

/// Move `d` along `want` at `speed`, turning towards it, round steep or wet
/// ground by turning aside. Keeps `doing` as `Bolt` when it is one.
fn run(d: &mut Deer, want: Vec2, speed: f32, turn: f32, dt: f32, map: &Map) {
    let want = want.normalize_or(d.facing);
    d.facing = turn_toward(d.facing, want, turn * dt);
    // Walk only once nearly facing the way: a deer turns on the spot first.
    if d.facing.dot(want) < 0.5 {
        return;
    }
    let reach = speed * dt;
    for angle in [0.0f32, 0.5, -0.5, 1.0, -1.0, 1.6, -1.6] {
        let dir = Vec2::from_angle(angle).rotate(d.facing);
        let to = d.pos + dir * reach.max(STEP * 0.5);
        if passable(map, d.pos, to) {
            d.pos += dir * reach;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn run_for(w: &mut Wildlife, map: &Map, seconds: f32, focus: Vec2, threats: &[Vec2]) {
        for _ in 0..(seconds * 20.0) as usize {
            w.advance(0.05, focus, 30.0, map, |_| true, |_| false, threats);
        }
    }

    #[test]
    fn cells_have_herds_where_the_land_suits_and_the_same_each_time() {
        let map = land(Habitat::Grassland);
        let (mut a, mut b) = (Wildlife::default(), Wildlife::default());
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut a, &map, 0.1, focus, &[]);
        run_for(&mut b, &map, 0.1, focus, &[]);
        assert!(!a.seen().is_empty());
        assert_eq!(a.seen().len(), b.seen().len());
        let bog = land(Habitat::PeatBog);
        let mut c = Wildlife::default();
        run_for(&mut c, &bog, 0.5, focus, &[]);
        assert!(c.seen().is_empty());
    }

    #[test]
    fn a_painted_map_is_judged_by_its_land() {
        let focus = Vec2::new(64.0, 64.0);
        let mut map = land(Habitat::Grassland);
        map.habitat.clear();
        let mut meadow = Wildlife::default();
        run_for(&mut meadow, &map, 0.1, focus, &[]);
        assert!(!meadow.seen().is_empty());
        map.land = vec![Land::Waste as u8; (map.tiles * map.tiles) as usize];
        let mut waste = Wildlife::default();
        run_for(&mut waste, &map, 0.1, focus, &[]);
        assert!(waste.seen().is_empty());
    }

    #[test]
    fn deer_graze_and_wander_near_home_on_dry_ground() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 90.0, focus, &[]);
        assert!(!w.seen().is_empty());
        for d in w.seen() {
            assert!(map.height_at(d.pos.x, d.pos.y) > map.water_level());
        }
        let kinds: HashSet<_> = w.seen().iter().map(|s| format!("{:?}", s.doing)).collect();
        assert!(!kinds.contains("Bolt"));
        for h in w.herds.values() {
            for d in &h.deer {
                assert!(d.pos.distance(h.home) < ROAM + SCATTER + 6.0);
            }
        }
    }

    #[test]
    fn a_soldier_near_sends_the_herd_running_away_and_it_settles_after() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 1.0, focus, &[]);
        let before: Vec<Vec2> = w
            .herds
            .values()
            .next()
            .unwrap()
            .deer
            .iter()
            .map(|d| d.pos)
            .collect();
        let key = *w.herds.keys().next().unwrap();
        let mid = w.herds[&key].centre();
        let threat = mid + Vec2::new(6.0, 0.0);
        run_for(&mut w, &map, 2.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.deer.iter().all(|d| d.doing == Doing::Bolt));
        assert!(herd.centre().distance(threat) > mid.distance(threat) + 3.0);
        assert!(herd.deer.iter().zip(&before).all(|(d, b)| d.pos != *b));
        // No threat any more: they stop, stand alert and go back to grazing.
        run_for(&mut w, &map, 40.0, focus, &[]);
        let herd = &w.herds[&key];
        assert!(herd.deer.iter().all(|d| d.doing != Doing::Bolt));
    }

    #[test]
    fn herds_far_from_the_camera_are_forgotten() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(30.0, 30.0), &[]);
        let near = w.herds.len();
        assert!(near > 0);
        run_for(&mut w, &map, 0.1, Vec2::new(100.0, 100.0), &[]);
        assert!(
            w.herds
                .keys()
                .all(|&(x, y)| (x * CELL - 100).abs() < 120 && (y * CELL - 100).abs() < 120)
        );
        assert!(w.herds.keys().all(|&(x, y)| x > 0 || y > 0));
    }

    #[test]
    fn no_herd_stands_near_a_building() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        for _ in 0..4 {
            w.advance(
                0.05,
                focus,
                30.0,
                &map,
                |_| true,
                |p| p.distance(focus) < 30.0,
                &[],
            );
        }
        assert!(w.seen().iter().all(|d| d.pos.distance(focus) > 20.0));
    }

    #[test]
    fn a_deer_turns_before_it_walks_and_never_steps_into_water() {
        let map = land(Habitat::Grassland);
        let start = (20..110)
            .flat_map(|x| (20..110).map(move |y| Vec2::new(x as f32 + 0.5, y as f32 + 0.5)))
            .find(|&p| open(&map, p))
            .expect("some dry ground");
        let mut d = Deer {
            pos: start,
            facing: Vec2::X,
            doing: Doing::Walk,
            left: 5.0,
            goal: Vec2::ZERO,
            fresh: false,
            phase: 0.0,
            scale: 1.0,
        };
        run(&mut d, Vec2::NEG_X, WALK, TURN_WALK, 0.05, &map);
        assert_eq!(d.pos, start);
        assert!(d.facing.x < 1.0);
        let mut moved = false;
        for _ in 0..400 {
            run(&mut d, Vec2::new(-1.0, 0.3), RUN, TURN_RUN, 0.05, &map);
            moved |= d.pos != start;
            assert!(open(&map, d.pos), "{:?}", d.pos);
        }
        assert!(moved);
    }

    #[test]
    fn a_zoomed_out_camera_keeps_its_herds_instead_of_making_them_anew() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        let frame =
            |w: &mut Wildlife| w.advance(0.05, focus, 110.0, &map, |_| true, |_| false, &[]);
        frame(&mut w);
        let keys: HashSet<_> = w.herds.keys().copied().collect();
        let first: Vec<Vec2> = w.seen().iter().map(|s| s.pos).collect();
        assert!(!first.is_empty());
        for _ in 0..600 {
            frame(&mut w);
            assert_eq!(w.herds.keys().copied().collect::<HashSet<_>>(), keys);
        }
        let later: Vec<Vec2> = w.seen().iter().map(|s| s.pos).collect();
        assert_eq!(first.len(), later.len());
        assert!(first.iter().zip(&later).any(|(a, b)| a != b));
    }

    #[test]
    fn a_wary_herd_is_not_startled_again_by_a_soldier_that_is_only_nearish() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
        let mut herd = w.herds.values().next().unwrap().clone();
        herd.bolt = 0.0;
        herd.wary = CALM;
        let at = herd.centre() + Vec2::new(SPOOK * 0.7, 0.0);
        step(&mut herd, 0.05, &map, &|_| true, &[at]);
        assert!(herd.bolt <= 0.0);
        assert_eq!(herd.wary, CALM, "a soldier within SAFE keeps it wary");
        let at = herd.centre() + Vec2::new(SPOOK * 0.3, 0.0);
        step(&mut herd, 0.05, &map, &|_| true, &[at]);
        assert!(herd.bolt > 0.0);
    }
}
