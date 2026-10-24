//! Busy bases, stage 3b (docs/34-ambient-life.md): a worker from each
//! finished workplace now and then walks to another building of the same
//! owner, stands at its door a while, and walks home. They are a picture:
//! not selectable, not in the simulation, in nobody's way, and they cost no
//! supply. They follow the buildings the viewer sees and the effects clock,
//! and walk straight between doors, so a trip that would cross a building,
//! water or a steep slope is never taken.

use std::collections::HashMap;

use glam::Vec2;

/// How far from the camera's focus walkers are kept, in tiles, how far past
/// a wall their door stands, and how far above the water the ground must
/// be for them to walk on it.
pub const REACH: f32 = 60.0;
pub const DOOR: f32 = 0.5;
/// How far past `REACH` buildings are still listed as obstacles, so one
/// that juts into the disc from beyond it is not walked through, in tiles.
pub const OBSTACLE_MARGIN: f32 = 12.0;
pub const SHORE: f32 = 0.05;
/// How fast a walker goes, in tiles a second, and how far from his own door
/// he will go.
const SPEED: f32 = 1.1;
const FAR: f32 = 16.0;
/// How long he stands at a door before setting off again, in seconds: the
/// least, and how much more at most.
const STAY: (f32, f32) = (4.0, 9.0);
/// How far a walker's path keeps clear of walls, in tiles.
const CLEAR: f32 = 0.25;
/// Walkers kept at once, at most.
pub const MOST: usize = 160;
/// How far apart the ground is looked at along a path, in tiles, and the
/// steepest climb over one such step a walker will take.
const STEP: f32 = 0.4;
const STEEPEST: f32 = 0.3;

/// A building a walker can start from or go to, as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Home {
    /// A number that stays with the building (its handle's index and
    /// generation).
    pub id: u64,
    pub owner: u8,
    /// Whether a walker lives here (a hall or a workplace).
    pub lives: bool,
    /// Whether a walker may go here. A wall, a tower or a building going up
    /// is only in the way.
    pub visit: bool,
    /// The model his walker is drawn with, as the game numbers models.
    pub worker: usize,
    /// The middle of the footprint and half its sides.
    pub centre: Vec2,
    pub half: Vec2,
    /// Where its door stands, just outside the wall.
    pub door: Vec2,
}

impl Home {
    fn blocks(&self, p: Vec2) -> bool {
        let d = (p - self.centre).abs();
        d.x < self.half.x + CLEAR && d.y < self.half.y + CLEAR
    }
}

/// A walker as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub pos: Vec2,
    pub facing: Vec2,
    pub walking: bool,
    /// Keeps walkers' steps out of time, 0 to 1.
    pub phase: f32,
    pub owner: u8,
    pub worker: usize,
}

#[derive(Clone, Copy, Debug)]
struct Walker {
    pos: Vec2,
    facing: Vec2,
    owner: u8,
    worker: usize,
    /// Where he is going, if on a trip, and the way he faces on arriving.
    to: Option<Vec2>,
    arrive: Vec2,
    /// Whether the trip in hand goes out (else home).
    outbound: bool,
    /// Whether he waits at a door away from home.
    away: bool,
    /// Seconds he stands still before the next trip.
    wait: f32,
    phase: f32,
}

#[derive(Default)]
pub struct Walkers {
    walkers: HashMap<u64, Walker>,
    rng: u32,
}

impl Walkers {
    fn unit(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x6c07_8965;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn stay(&mut self) -> f32 {
        STAY.0 + self.unit() * STAY.1
    }

    /// Forget everyone (a new match).
    pub fn clear(&mut self) {
        self.walkers.clear();
    }

    /// Everyone to draw.
    pub fn seen(&self) -> impl Iterator<Item = Seen> + '_ {
        self.walkers.values().map(|w| Seen {
            pos: w.pos,
            facing: w.facing,
            walking: w.to.is_some(),
            phase: w.phase,
            owner: w.owner,
            worker: w.worker,
        })
    }

    /// Whether the straight way from `a` to `b` is clear: dry flat ground,
    /// not through any building.
    fn clear_way(
        a: Vec2,
        b: Vec2,
        homes: &[Home],
        walkable: &impl Fn(Vec2) -> bool,
        climb: &impl Fn(Vec2, Vec2) -> f32,
    ) -> bool {
        let steps = ((b - a).length() / STEP).ceil().max(1.0) as u32;
        let mut last = a;
        for k in 1..=steps {
            let p = a.lerp(b, k as f32 / steps as f32);
            if homes.iter().any(|h| h.blocks(p)) || !walkable(p) || climb(last, p) > STEEPEST {
                return false;
            }
            last = p;
        }
        true
    }

    /// A door for a walker from `home` to go to, and the way to face there:
    /// another building of his owner within [`FAR`], by a clear way.
    fn pick(
        &mut self,
        home: &Home,
        homes: &[Home],
        walkable: &impl Fn(Vec2) -> bool,
        climb: &impl Fn(Vec2, Vec2) -> f32,
    ) -> Option<(Vec2, Vec2)> {
        let near: Vec<&Home> = homes
            .iter()
            .filter(|h| h.visit && h.id != home.id && h.owner == home.owner)
            .filter(|h| h.door.distance(home.door) <= FAR)
            .collect();
        if near.is_empty() {
            return None;
        }
        // Start from a random one so the same neighbour is not always it.
        let first = (self.unit() * near.len() as f32) as usize;
        (0..near.len())
            .map(|k| near[(first + k) % near.len()])
            .find(|h| Self::clear_way(home.door, h.door, homes, walkable, climb))
            .map(|h| (h.door, (h.centre - h.door).normalize_or(Vec2::Y)))
    }

    /// Move the walkers on by `dt` seconds of effects time. `homes` are the
    /// buildings the viewer sees now; `walkable` says whether the ground is
    /// dry and not too rough at a point, and `climb` how much it rises from
    /// one point to the next. A walker whose home is not in `homes` (out of
    /// view, destroyed) is forgotten at once, and so is one a building has
    /// been put in the way of.
    pub fn advance(
        &mut self,
        dt: f32,
        homes: &[Home],
        walkable: impl Fn(Vec2) -> bool,
        climb: impl Fn(Vec2, Vec2) -> f32,
    ) {
        let dt = dt.min(0.25);
        let index: HashMap<u64, usize> = (homes.iter().enumerate())
            .filter(|(_, h)| h.lives)
            .map(|(i, h)| (h.id, i))
            .collect();
        self.walkers.retain(|id, _| index.contains_key(id));
        for h in homes.iter().filter(|h| h.lives) {
            if self.walkers.len() >= MOST {
                break;
            }
            // A door with a building against it is no way out.
            if !self.walkers.contains_key(&h.id)
                && !homes.iter().any(|o| o.id != h.id && o.blocks(h.door))
            {
                let (wait, phase) = (self.stay(), self.unit());
                let facing = (h.door - h.centre).normalize_or(Vec2::Y);
                self.walkers.insert(
                    h.id,
                    Walker {
                        pos: h.door,
                        facing,
                        owner: h.owner,
                        worker: h.worker,
                        to: None,
                        arrive: facing,
                        outbound: false,
                        away: false,
                        wait,
                        phase,
                    },
                );
            }
        }
        let ids: Vec<u64> = self.walkers.keys().copied().collect();
        for id in ids {
            let home = homes[index[&id]];
            let mut w = self.walkers[&id];
            match w.to {
                None => {
                    w.wait -= dt;
                    if w.wait <= 0.0 {
                        if w.away {
                            if Self::clear_way(w.pos, home.door, homes, &walkable, &climb) {
                                w.to = Some(home.door);
                                w.arrive = (home.door - home.centre).normalize_or(Vec2::Y);
                                w.outbound = false;
                            } else {
                                // Shut in by a building put up meanwhile.
                                self.walkers.remove(&id);
                                continue;
                            }
                        } else if let Some((door, face)) =
                            self.pick(&home, homes, &walkable, &climb)
                        {
                            w.to = Some(door);
                            w.arrive = face;
                            w.outbound = true;
                        } else {
                            w.wait = STAY.0;
                        }
                    }
                }
                Some(to) => {
                    let way = to - w.pos;
                    let step = SPEED * dt;
                    if way.length() <= step {
                        w.pos = to;
                        w.to = None;
                        w.away = w.outbound;
                        w.wait = self.stay();
                        w.facing = w.arrive;
                    } else {
                        w.facing = way.normalize_or(w.facing);
                        w.pos += w.facing * step;
                        if homes.iter().any(|h| h.blocks(w.pos)) {
                            self.walkers.remove(&id);
                            continue;
                        }
                    }
                }
            }
            self.walkers.insert(id, w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(id: u64, x: f32, lives: bool) -> Home {
        Home {
            id,
            owner: 0,
            lives,
            visit: !lives,
            worker: 0,
            centre: Vec2::new(x, 10.0),
            half: Vec2::new(1.0, 1.0),
            door: Vec2::new(x, 11.6),
        }
    }

    fn flat() -> (impl Fn(Vec2) -> bool, impl Fn(Vec2, Vec2) -> f32) {
        (|_| true, |_, _| 0.0)
    }

    fn run(w: &mut Walkers, homes: &[Home], seconds: f32) {
        let (ok, climb) = flat();
        for _ in 0..(seconds * 20.0) as usize {
            w.advance(0.05, homes, &ok, &climb);
        }
    }

    #[test]
    fn a_workplace_gets_one_walker_who_goes_out_and_comes_home() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        assert_eq!(w.seen().count(), 1);
        let mut went = false;
        let mut came_home = false;
        for _ in 0..(120.0 * 20.0) as usize {
            run(&mut w, &homes, 0.05);
            let s = w.seen().next().unwrap();
            went |= s.pos.distance(homes[1].door) < 0.1;
            came_home |= went && s.pos.distance(homes[0].door) < 0.1;
        }
        assert!(went && came_home);
    }

    #[test]
    fn he_does_not_walk_through_a_building() {
        let mut wall = home(3, 13.0, false);
        wall.half = Vec2::new(1.0, 3.0);
        wall.door = Vec2::new(13.0, 14.0);
        let homes = [home(1, 10.0, true), home(2, 16.0, false), wall];
        let mut w = Walkers::default();
        // The building between them is a valid place to go but the far
        // one is shut off; he never ends up on the far side.
        for _ in 0..(200.0 * 20.0) as usize {
            run(&mut w, &homes, 0.05);
            let s = w.seen().next().unwrap();
            assert!(!wall.blocks(s.pos), "{:?}", s.pos);
        }
    }

    #[test]
    fn no_trip_over_water_or_a_cliff() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        for _ in 0..(100.0 * 20.0) as usize {
            w.advance(0.05, &homes, |_| false, |_, _| 0.0);
            assert!(!w.seen().next().unwrap().walking);
        }
        for _ in 0..(100.0 * 20.0) as usize {
            w.advance(0.05, &homes, |_| true, |_, _| 5.0);
            assert!(!w.seen().next().unwrap().walking);
        }
    }

    #[test]
    fn walkers_are_forgotten_when_their_home_is_not_seen() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        run(&mut w, &[homes[1]], 0.05);
        assert_eq!(w.seen().count(), 0);
    }

    #[test]
    fn a_building_put_in_his_way_does_not_trap_him() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        let mut blocked = false;
        for _ in 0..(200.0 * 20.0) as usize {
            let mut now = homes.to_vec();
            // Once he is standing at the far door, wall in the way back.
            if (w.seen().next()).is_some_and(|s| s.pos.distance(homes[1].door) < 0.1) {
                blocked = true;
                let mut wall = home(3, 13.0, false);
                wall.half = Vec2::new(0.5, 5.0);
                wall.visit = false;
                now.push(wall);
            }
            run(&mut w, &now, 0.05);
            if let Some(s) = w.seen().next() {
                assert!(!now.iter().any(|h| h.id == 3 && h.blocks(s.pos)));
            }
            if blocked && w.seen().count() == 0 {
                break;
            }
        }
        assert!(blocked);
    }

    #[test]
    fn a_door_with_a_building_against_it_gets_no_walker() {
        let mut next = home(2, 10.0, false);
        next.half = Vec2::new(3.0, 1.0);
        next.centre = Vec2::new(10.0, 12.5);
        let mut w = Walkers::default();
        run(&mut w, &[home(1, 10.0, true), next], 1.0);
        assert_eq!(w.seen().count(), 0);
    }

    #[test]
    fn a_lone_building_has_nowhere_to_go() {
        let homes = [home(1, 10.0, true)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 60.0);
        assert!(!w.seen().next().unwrap().walking);
    }

    #[test]
    fn at_most_so_many_walk() {
        let homes: Vec<Home> = (0..400).map(|i| home(i, i as f32 * 3.0, true)).collect();
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        assert_eq!(w.seen().count(), MOST);
    }
}
