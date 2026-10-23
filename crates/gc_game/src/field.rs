//! What a fight leaves lying on the field besides the fallen (who are the
//! puppets' business): the arrows that came down. Each volley is drawn as a
//! few arrows (`ARROWS_PER_VOLLEY`) arcing from the archers to where the
//! simulation aimed; the ones that come down in the open stay, stuck in the
//! ground at a steep slant, or in the body of a soldier they killed.
//!
//! Only a picture, like the arrows in flight: nothing here feeds back into
//! the simulation, so it uses floats, and a viewer who skips ticks (catching
//! up, seeking) simply has fewer arrows on the field.

use gc_fixed::FxVec2;
use gc_render::{BoxInstance, Cull};
use gc_sim::combat::MAX_VOLLEYS;
use gc_sim::events::VolleyId;
use glam::{Vec2, Vec3};

/// Arrows drawn per volley, and how far apart they fly, in tiles.
pub const ARROWS_PER_VOLLEY: usize = 6;
pub const ARROW_SPREAD: f32 = 1.6;
/// How far, in tiles, from its archers or from where it was aimed an arrow
/// caught by the trees stops.
pub const ARROW_CAUGHT_TILES: f32 = 1.0;
/// Peak of an arrow's arc as a fraction of the distance it flies.
const ARROW_ARC: f32 = 0.22;
/// An arrow in flight: its box, and colour.
pub const ARROW_SIZE: [f32; 3] = [0.05, 0.05, 0.7];
pub const ARROW_COLOR: [u8; 4] = [70, 52, 36, 0];
/// Spent arrows kept on the field at once; when it is full the oldest
/// vanish first. 24 bytes each, so 576 KB, and two boxes to draw.
pub const MAX_STUCK: usize = 24_576;

/// The shaft: its width and length, in tiles, and how much of the length is
/// buried. Arrows are bigger than life, so they can be seen from a
/// regiment's height.
const SHAFT: [f32; 2] = [0.045, 0.34];
const BURIED: f32 = 0.1;
/// The fletching on top, paler, and how far it leans back toward the
/// archers (an arrow comes down steeply, tail up).
const FLETCHING: [f32; 2] = [0.08, 0.14];
const LEAN: f32 = 0.04;
const SHAFT_COLOR: [u8; 4] = [92, 66, 42, 0];
const FLETCHING_COLOR: [u8; 4] = [228, 216, 178, 0];
/// An arrow in a body starts this high (tiles) instead of at the ground.
pub const IN_BODY: f32 = 0.2;

/// How one volley's arrows fly. In the air and where they come down are
/// drawn from this alike, so an arrow lands where it was seen to land.
#[derive(Clone, Copy, Debug)]
pub struct Flight {
    pub from: Vec2,
    pub aim: Vec2,
    /// Unit vector from the archers to the aim.
    pub dir: Vec2,
    pub length: f32,
    seed: u32,
    /// Whether the archers stand in a wood, and whether the aim is in one.
    out_of: bool,
    into: bool,
}

impl Flight {
    /// The flight of the volley loosed on `launch_tick`; `in_wood` says
    /// whether a point is in a wood.
    pub fn new(
        launch_tick: u32,
        from: FxVec2,
        aim: FxVec2,
        in_wood: impl Fn(FxVec2) -> bool,
    ) -> Flight {
        let (from_v, aim_v) = (crate::from_fx_vec(from), crate::from_fx_vec(aim));
        let travel = aim_v - from_v;
        Flight {
            from: from_v,
            aim: aim_v,
            dir: travel.normalize_or(Vec2::X),
            length: travel.length(),
            seed: launch_tick.wrapping_mul(0x9E37_79B9) ^ from.x.raw() as u32,
            out_of: in_wood(from),
            into: in_wood(aim),
        }
    }

    /// How far off the line to the aim arrow `k` flies.
    pub fn spread(&self, k: usize) -> Vec2 {
        Vec2::new(self.hash(k, 0), self.hash(k, 1)) * ARROW_SPREAD
    }

    /// How far along its flight, from 0 to 1, arrow `k` goes. The trees
    /// catch some of those loosed out of a wood or into one (as many as the
    /// simulation's trees do): they stop in the branches over the archers
    /// or over the target and hang there, so they never come down.
    pub fn stop(&self, k: usize) -> f32 {
        let catch = gc_sim::terrain::TREES_CATCH_PERCENT as f32 / 100.0;
        let caught = |axis| (self.hash(k, axis) + 1.0) * 0.5 < catch;
        let short = (ARROW_CAUGHT_TILES / self.length.max(f32::EPSILON)).min(0.5);
        if self.out_of && caught(2) {
            short
        } else if self.into && caught(3) {
            1.0 - short
        } else {
            1.0
        }
    }

    /// Where arrow `k` is when it has flown `t` (0 to 1) of the way.
    pub fn at(&self, k: usize, t: f32) -> Vec2 {
        self.from.lerp(self.aim, t) + self.spread(k)
    }

    /// How high arrow `k` flies over the ground at `t`: the arc, in tiles.
    pub fn arc(&self, t: f32) -> f32 {
        (t * std::f32::consts::PI).sin() * self.length * ARROW_ARC
    }

    /// Where arrow `k` comes down, unless the trees caught it.
    pub fn lands(&self, k: usize) -> Option<Vec2> {
        (self.stop(k) >= 1.0).then(|| self.at(k, 1.0))
    }

    /// A stable value in [-1, 1) per volley, arrow and axis.
    fn hash(&self, k: usize, axis: u32) -> f32 {
        crate::spread_hash(self.seed, k, axis)
    }
}

/// One arrow stuck where it came down.
#[derive(Clone, Copy, Debug)]
struct Stuck {
    pos: Vec2,
    /// The way it flew.
    dir: Vec2,
    /// The ground under it, in tiles, to tell whether it is on screen.
    ground: f32,
    /// How high it starts: 0 in the ground, `IN_BODY` in a body.
    base: f32,
}

/// Where a volley was loosed from, kept until it comes down: the
/// simulation's `Landed` event says where it was aimed but not from where.
#[derive(Clone, Copy, Debug)]
struct Loosed {
    launch_tick: u32,
    from: FxVec2,
    shots: u16,
}

/// The arrows stuck in the field, a ring buffer: the oldest are replaced
/// first. Allocated once.
pub struct Arrows {
    stuck: Vec<Stuck>,
    capacity: usize,
    next: usize,
    /// The volleys in the air, by the pool slot they fly in.
    loosed: Vec<Option<Loosed>>,
}

impl Arrows {
    pub fn new(capacity: usize) -> Arrows {
        Arrows {
            stuck: Vec::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
            next: 0,
            loosed: vec![None; MAX_VOLLEYS],
        }
    }

    /// Note that volley `id` was loosed from `from`, `shots` strong.
    pub fn loosed(&mut self, id: VolleyId, from: FxVec2, shots: u16) {
        if let Some(slot) = self.loosed.get_mut(id.slot as usize) {
            *slot = Some(Loosed {
                launch_tick: id.launch_tick,
                from,
                shots,
            });
        }
    }

    /// Volley `id` came down at `at`: stick the arrows drawn for it that
    /// reached the ground, and give their flight (none if the volley was
    /// never seen loosed, as when a viewer skipped ticks). `in_wood` says
    /// whether a point is in a wood; `ground` gives the height of dry ground
    /// there, and none for water, where arrows vanish.
    pub fn landed(
        &mut self,
        id: VolleyId,
        at: FxVec2,
        in_wood: impl Fn(FxVec2) -> bool,
        ground: impl Fn(Vec2) -> Option<f32>,
    ) -> Option<Flight> {
        let l = self
            .loosed
            .get(id.slot as usize)
            .copied()
            .flatten()
            .filter(|l| l.launch_tick == id.launch_tick)?;
        let flight = Flight::new(l.launch_tick, l.from, at, in_wood);
        for k in 0..ARROWS_PER_VOLLEY.min(usize::from(l.shots)) {
            if let Some((pos, ground)) = flight.lands(k).and_then(|p| Some((p, ground(p)?))) {
                self.stick(pos, flight.dir, ground, 0.0);
            }
        }
        Some(flight)
    }

    pub fn len(&self) -> usize {
        self.stuck.len()
    }

    /// Stick an arrow that flew along `dir` in the ground at `pos`, `ground`
    /// tiles up, or (with `base` at `IN_BODY`) in a body lying there.
    pub fn stick(&mut self, pos: Vec2, dir: Vec2, ground: f32, base: f32) {
        let arrow = Stuck {
            pos,
            dir,
            ground,
            base,
        };
        if self.stuck.len() < self.capacity {
            self.stuck.push(arrow);
        } else {
            self.stuck[self.next] = arrow;
            self.next = (self.next + 1) % self.capacity;
        }
    }

    /// Append the arrows `cull` sees as two boxes each, a shaft and its
    /// fletching leaning back toward the archers, until `out` holds `limit`.
    /// The newest go first, so when room runs short the oldest are the
    /// ones left out.
    pub fn push(&self, cull: &Cull, limit: usize, out: &mut Vec<BoxInstance>) {
        // Once the ring has wrapped, those before `next` are the newest.
        let (wrapped, rest) = self.stuck.split_at(self.next);
        for a in wrapped.iter().rev().chain(rest.iter().rev()) {
            if out.len() + 2 > limit {
                return;
            }
            let at = Vec3::new(a.pos.x, a.ground + a.base + SHAFT[1] * 0.5, a.pos.y);
            if !cull.sees(at, SHAFT[1] + LEAN) {
                continue;
            }
            // No two alike: a little taller or shorter, steeper or slanted.
            let h = hash(a.pos);
            let size = 0.85 + 0.3 * unit(h);
            let side = Vec2::new(-a.dir.y, a.dir.x);
            let lean = -a.dir * (LEAN * (0.7 + 0.6 * unit(h >> 8)))
                + side * (LEAN * 0.5 * (unit(h >> 16) - 0.5));
            out.push(BoxInstance {
                pos: a.pos.to_array(),
                facing: a.dir.to_array(),
                size: [SHAFT[0], SHAFT[1] * size, SHAFT[0]],
                color: SHAFT_COLOR,
                lift: a.base - BURIED,
            });
            out.push(BoxInstance {
                pos: (a.pos + lean).to_array(),
                facing: a.dir.to_array(),
                size: [FLETCHING[0], FLETCHING[1] * size, FLETCHING[0]],
                color: FLETCHING_COLOR,
                lift: a.base - BURIED + SHAFT[1] * size * 0.75,
            });
        }
    }
}

/// A well-mixed hash of a position.
fn hash(p: Vec2) -> u32 {
    let mut x = p.x.to_bits().wrapping_mul(0x9E37_79B9) ^ p.y.to_bits().wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x
}

/// 0 to 1 from the low byte of `h`.
fn unit(h: u32) -> f32 {
    (h & 0xff) as f32 / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::Fx;

    fn fx(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn arrows_fly_where_they_land() {
        let f = Flight::new(40, fx(10, 10), fx(30, 10), |_| false);
        assert!((f.dir - Vec2::X).length() < 1e-6 && (f.length - 20.0).abs() < 1e-3);
        for k in 0..ARROWS_PER_VOLLEY {
            // In the open nothing stops short, and the end of the flight is
            // the landing.
            assert_eq!(f.stop(k), 1.0);
            let lands = f.lands(k).expect("comes down");
            assert!(lands.distance(f.at(k, 1.0)) < 1e-6);
            assert!(f.spread(k).abs().max_element() <= ARROW_SPREAD);
        }
        // Not all alike, but the same every time.
        assert_ne!(f.spread(0), f.spread(1));
        assert_eq!(
            f.spread(3),
            Flight::new(40, fx(10, 10), fx(30, 10), |_| true).spread(3)
        );
    }

    #[test]
    fn the_trees_catch_arrows_into_and_out_of_a_wood() {
        let open = Flight::new(7, fx(0, 0), fx(30, 0), |_| false);
        let wood = Flight::new(7, fx(0, 0), fx(30, 0), |_| true);
        let caught = |f: &Flight| (0..400).filter(|&k| f.stop(k) < 1.0).count();
        assert_eq!(caught(&open), 0);
        // About half of them with a wood at each end (30% + 30% of the rest).
        let n = caught(&wood);
        assert!((160..280).contains(&n), "{n} of 400");
        for k in 0..400 {
            let t = wood.stop(k);
            assert_eq!(wood.lands(k).is_some(), t >= 1.0);
            // They stop a tile from either end, or not at all.
            let t_len = t * wood.length;
            assert!(t >= 1.0 || (t_len - 1.0).abs() < 1e-3 || (t_len - 29.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_volley_that_comes_down_leaves_its_arrows() {
        let mut a = Arrows::new(64);
        let id = VolleyId {
            slot: 5,
            launch_tick: 90,
        };
        // Never seen loosed: nothing to say where it came from.
        assert!(a.landed(id, fx(30, 10), |_| false, |_| Some(0.0)).is_none());
        a.loosed(id, fx(10, 10), 24);
        // Another volley now flies in the slot: the first one's arrows are
        // not this one's.
        let later = VolleyId {
            launch_tick: 130,
            ..id
        };
        assert!(
            a.landed(later, fx(30, 10), |_| false, |_| Some(0.0))
                .is_none()
        );
        let flight = a
            .landed(id, fx(30, 10), |_| false, |_| Some(2.0))
            .expect("loosed");
        assert!((flight.dir - Vec2::X).length() < 1e-6);
        assert_eq!(a.len(), ARROWS_PER_VOLLEY);
        assert!(
            a.stuck
                .iter()
                .all(|s| s.pos.distance(Vec2::new(30.0, 10.0)) < 3.0)
        );
        assert!(a.stuck.iter().all(|s| s.ground == 2.0 && s.base == 0.0));
        // Into the water they vanish, and a thin volley sends fewer.
        a.loosed(id, fx(10, 10), 2);
        a.landed(id, fx(30, 10), |_| false, |_| None);
        assert_eq!(a.len(), ARROWS_PER_VOLLEY);
        a.landed(id, fx(30, 10), |_| false, |_| Some(0.0));
        assert_eq!(a.len(), ARROWS_PER_VOLLEY + 2);
    }

    #[test]
    fn the_oldest_arrows_make_room() {
        let mut a = Arrows::new(3);
        for i in 0..5 {
            a.stick(Vec2::new(i as f32, 0.0), Vec2::X, 0.0, 0.0);
        }
        assert_eq!(a.len(), 3);
        let mut xs: Vec<f32> = a.stuck.iter().map(|s| s.pos.x).collect();
        xs.sort_by(f32::total_cmp);
        assert_eq!(xs, [2.0, 3.0, 4.0]);
        // Short of room, the newest are drawn and the oldest left out.
        let mut out = Vec::new();
        a.push(&Cull::EVERYTHING, 4, &mut out);
        let drawn: Vec<f32> = out.iter().step_by(2).map(|b| b.pos[0]).collect();
        assert_eq!(drawn, [4.0, 3.0]);
    }

    #[test]
    fn stuck_arrows_draw_two_boxes_each_within_the_limit() {
        let mut a = Arrows::new(8);
        for i in 0..5 {
            a.stick(Vec2::new(i as f32, 2.0), Vec2::NEG_Y, 1.0, 0.0);
        }
        a.stick(Vec2::new(9.0, 9.0), Vec2::NEG_Y, 1.0, IN_BODY);
        let mut out = Vec::new();
        a.push(&Cull::EVERYTHING, usize::MAX, &mut out);
        assert_eq!(out.len(), 12);
        // Partly buried: the shaft starts below the ground, the fletching
        // above the shaft's top and leaning back toward the archers (the
        // arrow flew toward -y, so its tail is toward +y).
        // The newest first: the arrow in a body, then the last in the ground.
        assert!(out[2].lift < 0.0);
        assert!(out[3].lift > out[2].lift + 0.1);
        assert!(out[3].pos[1] > out[2].pos[1]);
        assert!(out[0].lift > out[2].lift + 0.1);
        // Short of room, it stops early and never splits an arrow.
        out.clear();
        a.push(&Cull::EVERYTHING, 5, &mut out);
        assert_eq!(out.len(), 4);
    }
}
