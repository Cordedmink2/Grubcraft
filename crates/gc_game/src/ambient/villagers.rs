//! Villagers, stage 5b (docs/34-ambient-life.md): the Grubfolk who live in
//! the hamlets of stage 5a. Farmers, porters, children and a dog or two go
//! about their day between the doors, stalls and haystacks: they walk,
//! work, rest, and go in and come out of the cottages. They stop and look
//! at an army that passes, and when a fight breaks out near their hamlet
//! they run to the nearest door, shut themselves in and stay there until
//! it is quiet again. A picture only: not in the simulation, not saved, not
//! sent, in nobody's way (nothing blocks soldiers and the villagers do not
//! feel them).
//!
//! A hamlet's people are made when the camera comes near it (from the
//! hamlet's own site, so each has the same people) and forgotten when it
//! goes away again.

use std::collections::{HashMap, HashSet};

use glam::Vec2;

use super::hamlets::{Hamlets, Node, Rng, Size, Spot};
use super::wildlife::{reach, turn_toward};

/// How far past the camera's reach a hamlet is kept (tiles).
const KEEP: f32 = 14.0;
/// A fight this near a hamlet's middle (tiles) sends everyone indoors, and
/// they come out this many seconds after the last one near.
const ALARM: f32 = 40.0;
const CALM: f32 = 10.0;
/// An army this near a villager (tiles) makes them stop and look.
const WATCH: f32 = 15.0;
/// How close to a hamlet's middle (the well, or the ring of stalls) a villager
/// walks, in tiles: a chord nearer than this bends round it.
const MIDDLE: f32 = 1.5;
/// How quickly a villager turns, in radians a second.
const TURN: f32 = 4.0;
/// Seconds a villager stands about, the least and how much more at most.
const REST: (f32, f32) = (1.5, 5.0);
/// Seconds of work at a haystack, a stall.
const WORK: (f32, f32) = (6.0, 14.0);
const PAUSE: (f32, f32) = (3.0, 6.0);
/// Seconds spent indoors after going in, the least and how much more.
const INDOORS: (f32, f32) = (6.0, 14.0);
/// How a villager at the door of a cottage that has just opened looks: the
/// stagger between people coming out when a fight is over, in seconds.
const EMERGE: (f32, f32) = (0.5, 5.0);

/// The sort of person, for the model and the way of going about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Farmer,
    Porter,
    Child,
    Dog,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Farmer, Kind::Porter, Kind::Child, Kind::Dog];

    /// The model's name in the pack.
    pub fn model(self) -> &'static str {
        match self {
            Kind::Farmer => "FolkFarmer",
            Kind::Porter => "FolkPorter",
            Kind::Child => "FolkChild",
            Kind::Dog => "Dog",
        }
    }

    /// Where the kind is in [`Kind::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Tiles a second, walking and running (these match the clips' strides).
    fn pace(self) -> (f32, f32) {
        match self {
            Kind::Farmer | Kind::Porter => (0.9, 2.1),
            Kind::Child => (0.65, 1.5),
            Kind::Dog => (1.1, 3.0),
        }
    }

    /// The chance of going in at a door rather than walking on.
    fn homebody(self) -> f32 {
        match self {
            Kind::Farmer => 0.5,
            Kind::Porter => 0.35,
            Kind::Child => 0.2,
            Kind::Dog => 0.0,
        }
    }
}

/// The people of a hamlet of this size.
fn crowd(size: Size) -> &'static [Kind] {
    use Kind::*;
    match size {
        Size::Village => &[Farmer, Porter, Child, Dog],
        Size::Town => &[Farmer, Farmer, Porter, Porter, Child, Child, Dog],
        Size::Bazaar => &[Porter, Porter, Porter, Porter, Farmer, Child, Child, Dog],
    }
}

/// What a villager is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    Stand,
    Walk,
    Run,
    /// The kind's work: hoeing, mopping a brow, jumping, sniffing.
    Work,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Task {
    /// Standing about for `left` seconds.
    Rest { left: f32 },
    /// Going to node `hop` on the way to node `goal`.
    Go { hop: usize, goal: usize, run: bool },
    /// Working at a node for `left` seconds.
    Work { left: f32 },
    /// Looking at an army; `left` seconds until they may go on once it has
    /// gone.
    Watch { left: f32 },
    /// Out of sight indoors at node `door`: `left` seconds, which stop while
    /// there is an alarm.
    Inside { door: usize, left: f32 },
}

#[derive(Clone, Debug)]
struct Person {
    kind: Kind,
    pos: Vec2,
    facing: Vec2,
    /// The node it last stood at.
    at: usize,
    task: Task,
    rng: Rng,
    phase: f32,
}

#[derive(Clone, Debug)]
struct Group {
    centre: Vec2,
    nodes: Vec<Node>,
    people: Vec<Person>,
    /// Seconds of alarm left.
    alarm: f32,
}

/// A villager as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub kind: Kind,
    pub pos: Vec2,
    pub facing: Vec2,
    pub doing: Doing,
    pub phase: f32,
}

#[derive(Default)]
pub struct Villagers {
    groups: HashMap<usize, Group>,
    /// Sites looked at that have nobody, so they are not looked at every
    /// frame.
    bare: HashSet<usize>,
    seen: Vec<Seen>,
}

/// What the viewer sees of the field's soldiers: where every regiment is, and
/// where the ones in melee are.
#[derive(Clone, Copy)]
pub struct Eyes<'a> {
    pub armies: &'a [Vec2],
    pub fights: &'a [Vec2],
}

impl Person {
    fn pick(&mut self, range: (f32, f32)) -> f32 {
        range.0 + (range.1 - range.0) * self.rng.unit()
    }

    /// The next node on the way from `from` to `goal`: the one next in
    /// bearing, the short way round.
    fn next_hop(from: usize, goal: usize, n: usize) -> usize {
        let forward = (goal + n - from) % n;
        if forward <= n - forward {
            (from + 1) % n
        } else {
            (from + n - 1) % n
        }
    }

    /// Choose where to go from node `at`: a node that suits the kind.
    fn choose(&mut self, nodes: &[Node]) -> Task {
        let n = nodes.len();
        if n < 2 {
            return Task::Rest {
                left: self.pick(REST),
            };
        }
        // Weighted by what the kind likes: farmers the haystacks, porters
        // the stalls, children and dogs anywhere.
        let mut weights: Vec<f32> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                if i == self.at {
                    return 0.0;
                }
                match (self.kind, node.spot) {
                    (Kind::Farmer, Spot::Haystack) => 4.0,
                    (Kind::Porter, Spot::Stall) => 4.0,
                    (Kind::Porter, Spot::Door) => 2.0,
                    (Kind::Farmer, Spot::Door) => 2.0,
                    (_, Spot::Well) => 2.0,
                    _ => 1.0,
                }
            })
            .collect();
        let total: f32 = weights.iter().sum();
        let mut roll = self.rng.unit() * total;
        let mut goal = (self.at + 1) % n;
        for (i, w) in weights.iter_mut().enumerate() {
            if *w > 0.0 && roll < *w {
                goal = i;
                break;
            }
            roll -= *w;
        }
        let run = self.kind == Kind::Child || (self.kind == Kind::Dog && self.rng.unit() < 0.4);
        Task::Go {
            hop: Person::next_hop(self.at, goal, n),
            goal,
            run,
        }
    }

    /// What it does on getting to node `node`.
    fn arrive(&mut self, nodes: &[Node], node: usize) -> Task {
        let place = nodes[node];
        self.at = node;
        self.facing = place.look;
        match place.spot {
            Spot::Door if self.rng.unit() < self.kind.homebody() => Task::Inside {
                door: node,
                left: self.pick(INDOORS),
            },
            Spot::Haystack if self.kind == Kind::Farmer => Task::Work {
                left: self.pick(WORK),
            },
            Spot::Stall if self.kind == Kind::Porter => Task::Work {
                left: self.pick(PAUSE),
            },
            _ if self.kind == Kind::Child || self.kind == Kind::Dog => Task::Work {
                left: self.pick((1.5, 4.0)),
            },
            _ => Task::Rest {
                left: self.pick(REST),
            },
        }
    }

    /// The door nearest the person: where to run in an alarm.
    fn nearest_door(&self, nodes: &[Node]) -> Option<usize> {
        nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.spot == Spot::Door)
            .min_by(|a, b| {
                a.1.at
                    .distance_squared(self.pos)
                    .total_cmp(&b.1.at.distance_squared(self.pos))
            })
            .map(|(i, _)| i)
    }

    fn step(&mut self, dt: f32, centre: Vec2, nodes: &[Node], alarm: bool, armies: &[Vec2]) {
        let n = nodes.len();
        if n == 0 {
            return;
        }
        // Fear first: everybody out of doors heads for the nearest door.
        if alarm {
            match self.task {
                Task::Inside { .. } => {}
                Task::Go {
                    goal, run: true, ..
                } if Some(goal) == self.nearest_door(nodes) => {}
                _ => {
                    if let Some(door) = self.nearest_door(nodes) {
                        // From where it stands: straight to that door, then in.
                        self.task = Task::Go {
                            hop: door,
                            goal: door,
                            run: true,
                        };
                    }
                }
            }
        }
        let watching = !alarm
            && !matches!(self.task, Task::Inside { .. })
            && self.kind != Kind::Dog
            && armies.iter().any(|a| a.distance(self.pos) < WATCH);
        if watching && !matches!(self.task, Task::Watch { .. }) {
            self.task = Task::Watch { left: 0.0 };
        }
        match self.task {
            Task::Rest { left } | Task::Work { left } => {
                if left <= dt {
                    self.task = self.choose(nodes);
                } else {
                    let left = left - dt;
                    self.task = match self.task {
                        Task::Work { .. } => Task::Work { left },
                        _ => Task::Rest { left },
                    };
                }
            }
            Task::Watch { left } => {
                if let Some(a) = armies
                    .iter()
                    .min_by(|a, b| {
                        a.distance_squared(self.pos)
                            .total_cmp(&b.distance_squared(self.pos))
                    })
                    .filter(|a| a.distance(self.pos) < WATCH)
                {
                    self.facing = turn_toward(self.facing, *a - self.pos, TURN * dt);
                    self.task = Task::Watch {
                        left: self.pick((1.0, 2.5)),
                    };
                } else if left <= dt {
                    self.task = Task::Rest {
                        left: self.pick((0.5, 2.0)),
                    };
                } else {
                    self.task = Task::Watch { left: left - dt };
                }
            }
            Task::Inside { door, left } => {
                if alarm {
                    // Keep the door shut.
                    self.task = Task::Inside {
                        door,
                        left: self.pick(EMERGE),
                    };
                } else if left <= dt {
                    self.at = door;
                    self.pos = nodes[door].at;
                    self.facing = -nodes[door].look;
                    self.task = Task::Rest {
                        left: self.pick((0.5, 2.0)),
                    };
                } else {
                    self.task = Task::Inside {
                        door,
                        left: left - dt,
                    };
                }
            }
            Task::Go { hop, goal, run } => {
                let (walk, dash) = self.kind.pace();
                let speed = if run { dash } else { walk };
                let mut aim = nodes[hop].at;
                // A chord that would cut through the well, or the ring of
                // stalls, bends round it.
                let line = aim - self.pos;
                let along = ((centre - self.pos).dot(line) / line.length_squared().max(1e-6))
                    .clamp(0.0, 1.0);
                let nearest = self.pos + line * along;
                if nearest.distance(centre) < MIDDLE && along > 0.0 && along < 1.0 {
                    let off = (nearest - centre)
                        .try_normalize()
                        .unwrap_or_else(|| line.perp().normalize_or(Vec2::X));
                    aim = centre + off * (MIDDLE + 0.4);
                }
                let to = aim - self.pos;
                let dist = to.length();
                if dist > 1e-3 {
                    self.facing = turn_toward(self.facing, to, TURN * dt);
                }
                if dist <= speed * dt && aim != nodes[hop].at {
                    // Round the middle; on to the node next time.
                    self.pos = aim;
                } else if dist <= speed * dt {
                    self.pos = nodes[hop].at;
                    self.at = hop;
                    if hop == goal {
                        self.task = if alarm && nodes[hop].spot == Spot::Door {
                            Task::Inside {
                                door: hop,
                                left: self.pick(EMERGE),
                            }
                        } else {
                            self.arrive(nodes, hop)
                        };
                    } else {
                        self.task = Task::Go {
                            hop: Person::next_hop(hop, goal, n),
                            goal,
                            run,
                        };
                    }
                } else {
                    self.pos += to / dist * speed * dt;
                }
            }
        }
    }

    fn seen(&self) -> Option<Seen> {
        let doing = match self.task {
            Task::Inside { .. } => return None,
            Task::Rest { .. } | Task::Watch { .. } => Doing::Stand,
            Task::Work { .. } => Doing::Work,
            Task::Go { run: false, .. } => Doing::Walk,
            Task::Go { run: true, .. } => Doing::Run,
        };
        Some(Seen {
            kind: self.kind,
            pos: self.pos,
            facing: self.facing,
            doing,
            phase: self.phase,
        })
    }
}

/// The people of site `i` of `hamlets`, standing at their places.
fn found(
    hamlets: &Hamlets,
    i: usize,
    alarm: bool,
    occupied: impl Fn((i32, i32)) -> bool,
) -> Option<Group> {
    let nodes = hamlets.nodes(i, occupied);
    let doors: Vec<usize> = (0..nodes.len())
        .filter(|&k| nodes[k].spot == Spot::Door)
        .collect();
    if nodes.len() < 3 || doors.is_empty() {
        return None;
    }
    let centre = hamlets.centre(i);
    let seed = (centre.x as u32).wrapping_mul(7919) ^ (centre.y as u32).wrapping_mul(104_729);
    let people = crowd(hamlets.size(i))
        .iter()
        .enumerate()
        .map(|(k, &kind)| {
            let mut rng = Rng::seeded(seed.wrapping_add(k as u32 * 0x9e37));
            // Made in a fight, a person is indoors behind a door of its own.
            let at = if alarm {
                doors[(rng.unit() * doors.len() as f32) as usize % doors.len()]
            } else {
                (rng.unit() * nodes.len() as f32) as usize % nodes.len()
            };
            let task = if alarm {
                Task::Inside {
                    door: at,
                    left: 0.0,
                }
            } else {
                Task::Rest {
                    left: 0.5 + 4.0 * rng.unit(),
                }
            };
            Person {
                kind,
                pos: nodes[at].at,
                facing: nodes[at].look,
                at,
                task,
                rng,
                phase: k as f32 * 0.37,
            }
        })
        .collect();
    Some(Group {
        centre,
        nodes,
        people,
        alarm: 0.0,
    })
}

impl Villagers {
    /// Forget everybody (a new match).
    pub fn clear(&mut self) {
        self.groups.clear();
        self.bare.clear();
        self.seen.clear();
    }

    /// Draw no villagers (the game is behind a menu, or the view is not the
    /// field's), keeping them as they are.
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The villagers to draw, as of the last `advance`.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Move the villagers of the hamlets near `focus` (the camera, `distance`
    /// away) on by `dt` seconds of effects time. `armies` are the places of
    /// the regiments the viewer sees and `fights` those of them in melee, as
    /// `eyes`; a hamlet's people are made without the cottages and stalls
    /// `occupied` says a player has built on.
    pub fn advance(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        hamlets: &Hamlets,
        eyes: Eyes,
        occupied: impl Fn((i32, i32)) -> bool,
    ) {
        let fights = eyes.fights;
        let dt = dt.min(0.25);
        let near = reach(distance) + super::hamlets::SPREAD;
        self.groups
            .retain(|_, g| g.centre.distance(focus) <= near + KEEP);
        for i in 0..hamlets.len() {
            let centre = hamlets.centre(i);
            if centre.distance(focus) > near
                || self.groups.contains_key(&i)
                || self.bare.contains(&i)
            {
                continue;
            }
            let alarm = fights.iter().any(|f| f.distance(centre) < ALARM);
            match found(hamlets, i, alarm, &occupied) {
                Some(g) => {
                    self.groups.insert(i, g);
                }
                // No door to live behind (every cottage is built over, or
                // the ground left none): nobody lives there.
                None => {
                    self.bare.insert(i);
                }
            }
        }
        for g in self.groups.values_mut() {
            if fights.iter().any(|f| f.distance(g.centre) < ALARM) {
                g.alarm = CALM;
            } else {
                g.alarm = (g.alarm - dt).max(0.0);
            }
            if dt > 0.0 {
                let (nodes, alarm) = (&g.nodes, g.alarm > 0.0);
                for p in &mut g.people {
                    p.step(dt, g.centre, nodes, alarm, eyes.armies);
                }
            }
        }
        self.seen.clear();
        for g in self.groups.values() {
            self.seen.extend(g.people.iter().filter_map(Person::seen));
        }
    }
}

#[cfg(test)]
#[allow(clippy::needless_range_loop)]
mod tests {
    use super::*;
    use crate::ambient::hamlets::tests::flat;

    const NOBODY: Eyes = Eyes {
        armies: &[],
        fights: &[],
    };

    /// The flat test map's village, town and bazaar, with their middles.
    fn world() -> (Hamlets, [Vec2; 3]) {
        let (map, sites) = flat();
        let at = |k: usize| Vec2::new(sites[k].1.0 as f32 + 0.5, sites[k].1.1 as f32 + 0.5);
        (Hamlets::new(&map), [at(0), at(1), at(2)])
    }

    fn free(_: (i32, i32)) -> bool {
        false
    }

    /// The group whose hamlet is at `at`.
    fn village(v: &Villagers, at: Vec2) -> &Group {
        v.groups.values().find(|g| g.centre == at).unwrap()
    }

    /// The villagers seen of the group at `at`.
    fn count_seen(v: &Villagers, at: Vec2) -> usize {
        village(v, at)
            .people
            .iter()
            .filter_map(Person::seen)
            .count()
    }

    /// Run `n` steps of 0.1 s with the camera at `focus`.
    fn run(v: &mut Villagers, h: &Hamlets, focus: Vec2, n: usize, eyes: Eyes) {
        for _ in 0..n {
            v.advance(0.1, focus, 30.0, h, eyes, free);
        }
    }

    fn door_spots(g: &Group) -> Vec<Vec2> {
        g.nodes
            .iter()
            .filter(|n| n.spot == Spot::Door)
            .map(|n| n.at)
            .collect()
    }

    #[test]
    fn each_size_has_its_people_and_only_near_the_camera() {
        let (h, at) = world();
        let want = [
            vec![Kind::Farmer, Kind::Porter, Kind::Child, Kind::Dog],
            vec![
                Kind::Farmer,
                Kind::Farmer,
                Kind::Porter,
                Kind::Porter,
                Kind::Child,
                Kind::Child,
                Kind::Dog,
            ],
            vec![
                Kind::Porter,
                Kind::Porter,
                Kind::Porter,
                Kind::Porter,
                Kind::Farmer,
                Kind::Child,
                Kind::Child,
                Kind::Dog,
            ],
        ];
        for (k, kinds) in want.iter().enumerate() {
            let mut v = Villagers::default();
            v.advance(0.1, at[k], 30.0, &h, NOBODY, free);
            let g = village(&v, at[k]);
            assert_eq!(&g.people.iter().map(|p| p.kind).collect::<Vec<_>>(), kinds);
            assert_eq!(count_seen(&v, at[k]), kinds.len());
            // Far off: nobody is kept.
            v.advance(0.1, Vec2::splat(-1.0e5), 30.0, &h, NOBODY, free);
            assert!(v.groups.is_empty() && v.seen().is_empty());
        }
    }

    #[test]
    fn people_go_about_among_the_places_and_stay_in_the_hamlet() {
        let (h, at) = world();
        for k in 0..3 {
            let mut v = Villagers::default();
            let (mut walked, mut worked, mut went_in) = (false, false, false);
            for _ in 0..1800 {
                v.advance(0.1, at[k], 30.0, &h, NOBODY, free);
                let g = village(&v, at[k]);
                for p in &g.people {
                    assert!(p.pos.distance(at[k]) < 11.0, "{p:?} wandered off");
                    // And never into the well or the middle of the stalls.
                    assert!(p.pos.distance(at[k]) > 1.2, "{p:?} is on the well");
                }
                let seen: Vec<Seen> = g.people.iter().filter_map(Person::seen).collect();
                walked |= seen.iter().any(|s| s.doing == Doing::Walk);
                worked |= seen.iter().any(|s| s.doing == Doing::Work);
                went_in |= seen.len() < g.people.len();
            }
            assert!(walked, "nobody walked at {k}");
            assert!(worked, "nobody worked at {k}");
            assert!(went_in, "nobody went in at {k}");
        }
    }

    #[test]
    fn the_places_to_stand_run_round_the_hamlet_by_bearing() {
        let (h, at) = world();
        let spots = |k: usize| -> Vec<Spot> {
            let nodes = h.nodes(k, free);
            let bearings: Vec<f32> = nodes.iter().map(|n| (n.at - at[k]).to_angle()).collect();
            assert!(bearings.windows(2).all(|w| w[0] <= w[1]));
            nodes.iter().map(|n| n.spot).collect()
        };
        let count = |s: &[Spot], spot: Spot| s.iter().filter(|&&x| x == spot).count();
        let (village, town, bazaar) = (spots(0), spots(1), spots(2));
        assert_eq!(count(&village, Spot::Door), 5);
        assert_eq!(count(&village, Spot::Well), 1);
        assert_eq!(count(&town, Spot::Stall), 3);
        assert_eq!(count(&town, Spot::Well), 0);
        assert_eq!(count(&bazaar, Spot::Stall), 7);
        assert!(count(&bazaar, Spot::Door) >= 4);
        // A player's building over the green hides the doors on it.
        let built = |t: (i32, i32)| t == (at[0].x as i32, at[0].y as i32);
        assert!(h.nodes(0, built).len() < h.nodes(0, free).len());
    }

    #[test]
    fn a_fight_sends_everyone_indoors_and_they_come_out_at_doors_when_it_is_quiet() {
        let (h, at) = world();
        for k in 0..3 {
            let mut v = Villagers::default();
            run(&mut v, &h, at[k], 50, NOBODY);
            let fight = at[k] + Vec2::new(25.0, 0.0);
            let fighting = Eyes {
                armies: &[fight],
                fights: &[fight],
            };
            run(&mut v, &h, at[k], 300, fighting);
            assert_eq!(count_seen(&v, at[k]), 0, "site {k}");
            // Still shut in a while after the fight has gone.
            run(&mut v, &h, at[k], 50, NOBODY);
            assert_eq!(count_seen(&v, at[k]), 0);
            // They come out one at a time, at their doors.
            let doors = door_spots(village(&v, at[k]));
            let mut was_out = vec![false; village(&v, at[k]).people.len()];
            for _ in 0..200 {
                v.advance(0.1, at[k], 30.0, &h, NOBODY, free);
                for (out, p) in was_out.iter_mut().zip(&village(&v, at[k]).people) {
                    if !*out && !matches!(p.task, Task::Inside { .. }) {
                        *out = true;
                        assert!(doors.contains(&p.pos), "{p:?} came out away from a door");
                    }
                }
            }
            assert!(was_out.iter().all(|&o| o), "somebody never came out");
        }
    }

    #[test]
    fn people_made_in_a_fight_start_indoors_behind_a_door() {
        let (h, at) = world();
        for k in 0..3 {
            let mut v = Villagers::default();
            let fight = at[k] + Vec2::new(10.0, 0.0);
            let eyes = Eyes {
                armies: &[fight],
                fights: &[fight],
            };
            v.advance(0.1, at[k], 30.0, &h, eyes, free);
            assert_eq!(count_seen(&v, at[k]), 0);
            let g = village(&v, at[k]);
            for p in &g.people {
                let Task::Inside { door, .. } = p.task else {
                    panic!("{p:?} is out");
                };
                assert_eq!(g.nodes[door].spot, Spot::Door);
            }
        }
    }

    #[test]
    fn a_runner_goes_to_the_nearest_door() {
        let (h, at) = world();
        let mut v = Villagers::default();
        run(&mut v, &h, at[0], 20, NOBODY);
        let fight = at[0] + Vec2::new(25.0, 0.0);
        run(
            &mut v,
            &h,
            at[0],
            1,
            Eyes {
                armies: &[],
                fights: &[fight],
            },
        );
        let g = village(&v, at[0]);
        for p in g
            .people
            .iter()
            .filter(|p| !matches!(p.task, Task::Inside { .. }))
        {
            let Task::Go { goal, run, .. } = p.task else {
                panic!("{p:?}");
            };
            assert!(run);
            assert_eq!(Some(goal), p.nearest_door(&g.nodes));
        }
    }

    #[test]
    fn an_army_passing_makes_them_stop_and_look() {
        let (h, at) = world();
        let mut v = Villagers::default();
        run(&mut v, &h, at[0], 20, NOBODY);
        let army = at[0] + Vec2::new(5.0, 0.0);
        run(
            &mut v,
            &h,
            at[0],
            80,
            Eyes {
                armies: &[army],
                fights: &[],
            },
        );
        for p in village(&v, at[0])
            .people
            .iter()
            .filter(|p| p.kind != Kind::Dog)
        {
            if matches!(p.task, Task::Inside { .. }) {
                continue;
            }
            assert!(matches!(p.task, Task::Watch { .. }), "{p:?}");
            let to = (army - p.pos).normalize();
            assert!(p.facing.dot(to) > 0.95, "{p:?}");
        }
        // And they go on again once it has gone by.
        run(&mut v, &h, at[0], 200, NOBODY);
        assert!(
            village(&v, at[0])
                .people
                .iter()
                .all(|p| !matches!(p.task, Task::Watch { .. }))
        );
    }

    #[test]
    fn nobody_cuts_through_the_well() {
        let (h, at) = world();
        // A long chord across a green: from one door to the one opposite.
        let nodes = h.nodes(0, free);
        let (a, b) = (0, nodes.len() / 2);
        let mut p = Person {
            kind: Kind::Farmer,
            pos: nodes[a].at,
            facing: Vec2::X,
            at: a,
            task: Task::Go {
                hop: b,
                goal: b,
                run: false,
            },
            rng: Rng::seeded(5),
            phase: 0.0,
        };
        let mut reached = false;
        for _ in 0..600 {
            p.step(0.1, at[0], &nodes, false, &[]);
            reached |= p.at == b;
            assert!(p.pos.distance(at[0]) >= MIDDLE - 0.05, "{:?}", p.pos);
        }
        assert!(reached);
    }

    #[test]
    fn the_same_hamlet_has_the_same_days() {
        let (h, at) = world();
        let (mut a, mut b) = (Villagers::default(), Villagers::default());
        run(&mut a, &h, at[0], 300, NOBODY);
        run(&mut b, &h, at[0], 300, NOBODY);
        let seen = |v: &Villagers| {
            village(v, at[0])
                .people
                .iter()
                .filter_map(Person::seen)
                .collect::<Vec<_>>()
        };
        assert_eq!(seen(&a), seen(&b));
    }
}
