//! Caravans, stage 6 of docs/34-ambient-life.md: the traffic between the
//! Grubfolk's hamlets. Along the open ground between two neighbouring
//! hamlets an ox cart with its drover, or a pair of porters with sacks on
//! their backs, goes from one to the other, waits a while to unload, and
//! comes back. When an army comes near they stop and watch it, and a fight
//! near them holds them where they are until it is quiet. A picture only:
//! not in the simulation, not saved, not sent, in nobody's way.
//!
//! The routes are worked out once, when the hamlets are laid out. Every
//! caravan keeps going all the time, wherever the camera is, so one seen
//! later is where it should be; only those near the camera are handed to
//! the renderer.

use gc_map::Map;
use glam::Vec2;

use super::hamlets::{Hamlets, Rng, Size, route_clear};
use super::villagers::{Eyes, Kind};
use super::wildlife::{reach, turn_toward};

/// The longest a route is, in tiles, and the least (a hamlet this near
/// another is not worth a cart). A hamlet has this many routes at most.
const LONGEST: f32 = 180.0;
const SHORTEST: f32 = 24.0;
const ROUTES_PER_HAMLET: usize = 2;
/// How far past the hamlet's models a caravan stops, in tiles: the cart's
/// middle, so its ox's nose (2.9 tiles ahead of it) and its turning circle
/// stay outside.
const PARK: f32 = 3.2;
/// A route may not pass this close to a third hamlet's models.
const GIVE_WAY: f32 = 3.0;
/// The radius of the cart's wheels in `oxcart.py`, in tiles, and how long its
/// Walk clip is, in seconds: two steps of the ox and one turn of the wheels.
const WHEEL_RADIUS: f32 = 0.38;
const WALK_SECONDS: f32 = 2.0;
/// The cart's pace, tiles a second: a wheel rolls its rim in one turn, so the
/// wheels roll without slipping.
pub const CART_PACE: f32 = std::f32::consts::TAU * WHEEL_RADIUS / WALK_SECONDS;
/// How fast a caravan turns about at the end of its route, radians a second:
/// slowly enough that the people on the arc round it walk at their pace.
const TURN_CART: f32 = 0.65;
const TURN_FILE: f32 = 0.55;
const TURN_ONE: f32 = 3.0;
/// Seconds spent unloading at the end of a route.
const UNLOAD: (f32, f32) = (8.0, 18.0);
/// An army this near a caravan makes it stop and look for `LOOK` seconds
/// (then it goes on, so a camp beside the road does not stop it for good),
/// and a fight this near holds it until it has been quiet for `CALM`
/// seconds.
const WATCH: f32 = 14.0;
const LOOK: f32 = 6.0;
const ALARM: f32 = 40.0;
const CALM: f32 = 6.0;
/// A caravan turns back when a player's building is this far ahead of it
/// (tiles), and waits this long (seconds) before it starts.
const BLOCK_LOOK: f32 = 3.5;
const BLOCKED: f32 = 3.0;
/// How far past the camera's reach a caravan is handed over (tiles).
const KEEP: f32 = 8.0;
/// How far along its route `--hamlet-demo` puts a caravan, in tiles, and
/// how far ahead of it the camera looks, so it comes by.
const DEMO_ALONG: f32 = 6.0;
const DEMO_AHEAD: f32 = 14.0;

/// A way between two hamlets, the stops at its ends in tiles.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Route {
    from: Vec2,
    to: Vec2,
    /// The sizes of the hamlets at the ends.
    sizes: (Size, Size),
}

impl Route {
    fn len(&self) -> f32 {
        self.from.distance(self.to)
    }

    fn at(&self, along: f32) -> Vec2 {
        self.from
            .lerp(self.to, (along / self.len()).clamp(0.0, 1.0))
    }

    fn heading(&self, forward: bool) -> Vec2 {
        let way = (self.to - self.from).normalize_or(Vec2::X);
        if forward { way } else { -way }
    }
}

/// What a caravan is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Load {
    /// An ox cart and the man who leads it.
    Cart,
    /// Porters in file, this many.
    Porters(u8),
}

#[derive(Clone, Debug)]
struct Train {
    route: usize,
    load: Load,
    /// Tiles from the route's start, and the way it is going.
    along: f32,
    forward: bool,
    /// Seconds still to wait at the end, and still to wait for calm.
    unload: f32,
    hold: f32,
    /// Seconds spent looking at the army that is near.
    looked: f32,
    /// Whether it stood still this step, for an army or a fight.
    still: bool,
    facing: Vec2,
    rng: Rng,
    phase: f32,
}

/// One piece of a caravan as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    /// The ox cart: its middle is the caravan's place.
    Cart,
    /// The man who leads the cart.
    Drover,
    /// A porter with his sack.
    Porter,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub piece: Piece,
    pub pos: Vec2,
    pub facing: Vec2,
    pub moving: bool,
    /// How fast to play the Walk clip, so the strides match the pace.
    pub rate: f32,
    pub phase: f32,
}

#[derive(Default)]
pub struct Caravans {
    routes: Vec<Route>,
    trains: Vec<Train>,
    seen: Vec<Seen>,
}

impl Caravans {
    /// The caravans for `hamlets` on `map`: each hamlet is joined to the
    /// nearest two others it has a clear way to.
    pub fn new(map: &Map, hamlets: &Hamlets) -> Caravans {
        let n = hamlets.len();
        let mut routes: Vec<Route> = Vec::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut links = vec![0usize; n];
        for i in 0..n {
            if hamlets.reach(i) <= 0.0 {
                continue;
            }
            let mut others: Vec<usize> = (0..n)
                .filter(|&j| j != i && hamlets.reach(j) > 0.0)
                .filter(|&j| {
                    (SHORTEST..=LONGEST).contains(&hamlets.centre(i).distance(hamlets.centre(j)))
                })
                .collect();
            others.sort_by(|&a, &b| {
                let d = |j: usize| hamlets.centre(i).distance(hamlets.centre(j));
                d(a).total_cmp(&d(b))
            });
            for j in others {
                if links[i] >= ROUTES_PER_HAMLET {
                    break;
                }
                let key = (i.min(j), i.max(j));
                if links[j] >= ROUTES_PER_HAMLET || pairs.contains(&key) {
                    continue;
                }
                let Some(route) = route_between(map, hamlets, i, j) else {
                    continue;
                };
                pairs.push(key);
                links[i] += 1;
                links[j] += 1;
                routes.push(route);
            }
        }
        let trains = routes
            .iter()
            .enumerate()
            .map(|(i, r)| Train::new(i, r))
            .collect();
        Caravans {
            routes,
            trains,
            seen: Vec::new(),
        }
    }

    /// Move every caravan on by `dt` seconds, and gather those within reach
    /// of the camera's `focus` for drawing.
    pub fn advance(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        eyes: Eyes,
        occupied: impl Fn((i32, i32)) -> bool,
    ) {
        self.seen.clear();
        let within = reach(distance) + KEEP;
        for t in &mut self.trains {
            let route = self.routes[t.route];
            t.step(dt, &route, eyes, &occupied);
            let pos = route.at(t.along);
            if pos.distance(focus) <= within {
                t.show(&route, &mut self.seen);
            }
        }
    }

    /// Stop drawing (the menu is open, or nothing is being watched).
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The pieces to draw this frame.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// How many routes there are.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.routes.len()
    }

    /// `--hamlet-demo`: set a caravan (a cart if there is one) just on its
    /// way from the first end of its route, and give the place to look at,
    /// a little ahead of it.
    pub fn demo(&mut self) -> Option<Vec2> {
        let i = self.trains.iter().position(|t| t.load == Load::Cart).or(
            if self.trains.is_empty() {
                None
            } else {
                Some(0)
            },
        )?;
        let route = self.routes[self.trains[i].route];
        let t = &mut self.trains[i];
        t.along = (route.len() * 0.3).min(DEMO_ALONG);
        t.forward = true;
        t.unload = 0.0;
        t.hold = 0.0;
        t.facing = route.heading(true);
        Some(route.at(t.along + DEMO_AHEAD))
    }
}

/// The way from hamlet `i` to hamlet `j`: stops just outside each, if
/// there is clear ground between them and no third hamlet in the way.
fn route_between(map: &Map, hamlets: &Hamlets, i: usize, j: usize) -> Option<Route> {
    let (ci, cj) = (hamlets.centre(i), hamlets.centre(j));
    let way = (cj - ci).normalize_or(Vec2::X);
    let from = ci + way * (hamlets.reach(i) + PARK);
    let to = cj - way * (hamlets.reach(j) + PARK);
    if from.distance(to) < SHORTEST * 0.5
        || (to - from).dot(way) <= 0.0
        || (0..hamlets.len()).any(|k| {
            k != i
                && k != j
                && near_segment(hamlets.centre(k), from, to) < hamlets.reach(k) + GIVE_WAY
        })
    {
        return None;
    }
    route_clear(map, from, to).then_some(Route {
        from,
        to,
        sizes: (hamlets.size(i), hamlets.size(j)),
    })
}

/// How far `p` is from the segment `a`-`b`.
fn near_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

impl Train {
    fn new(index: usize, route: &Route) -> Train {
        let mut rng = Rng::seeded(index as u32 * 7919 + route.len() as u32);
        // Carts mostly on the routes that serve a town or the bazaar, and now
        // and then between villages; the rest are walked by porters.
        let trade = |s: Size| matches!(s, Size::Town | Size::Bazaar);
        let cart = if trade(route.sizes.0) || trade(route.sizes.1) {
            0.8
        } else {
            0.25
        };
        let load = if rng.unit() < cart {
            Load::Cart
        } else {
            Load::Porters(if rng.unit() < 0.5 { 1 } else { 2 })
        };
        let forward = rng.unit() < 0.5;
        Train {
            route: index,
            load,
            along: route.len() * rng.unit(),
            forward,
            unload: 0.0,
            hold: 0.0,
            looked: 0.0,
            still: false,
            facing: route.heading(forward),
            phase: rng.unit() * 4.0,
            rng,
        }
    }

    fn pace(&self) -> f32 {
        match self.load {
            Load::Cart => CART_PACE,
            Load::Porters(_) => Kind::Porter.pace().0,
        }
    }

    /// Radians a second it turns about at the end of its route.
    fn turn_rate(&self) -> f32 {
        match self.load {
            Load::Cart => TURN_CART,
            Load::Porters(1) => TURN_ONE,
            Load::Porters(_) => TURN_FILE,
        }
    }

    fn step(&mut self, dt: f32, route: &Route, eyes: Eyes, occupied: &dyn Fn((i32, i32)) -> bool) {
        let pos = route.at(self.along);
        let near = |all: &[Vec2], reach: f32| all.iter().any(|a| a.distance(pos) < reach);
        let army = near(eyes.armies, WATCH);
        self.looked = if army { self.looked + dt } else { 0.0 };
        if near(eyes.fights, ALARM) {
            self.hold = CALM;
        }
        self.still = self.hold > 0.0 || (army && self.looked < LOOK);
        if self.still {
            self.hold = (self.hold - dt).max(0.0);
            return;
        }
        let heading = route.heading(self.forward);
        if self.unload > 0.0 {
            self.unload -= dt;
            // Turn about for the way back: slowly, the way a cart does.
            self.facing = turn_toward(self.facing, heading, self.turn_rate() * dt);
            return;
        }
        // A building put up across the way turns it back.
        let tile = pos + heading * BLOCK_LOOK;
        if occupied((tile.x.floor() as i32, tile.y.floor() as i32)) {
            self.forward = !self.forward;
            self.unload = BLOCKED;
            return;
        }
        let ahead = if self.forward {
            route.len() - self.along
        } else {
            self.along
        };
        let go = (self.pace() * dt).min(ahead);
        self.along += if self.forward { go } else { -go };
        self.facing = heading;
        if go >= ahead {
            self.forward = !self.forward;
            self.along = self.along.clamp(0.0, route.len());
            self.unload = UNLOAD.0 + (UNLOAD.1 - UNLOAD.0) * self.rng.unit();
        }
    }

    /// Whether it is turning about: waiting to unload, but not yet facing
    /// the way it will go.
    fn turning(&self, route: &Route) -> bool {
        self.unload > 0.0 && self.facing.dot(route.heading(self.forward)) < 0.99
    }

    /// Whether the people and the ox are on their feet: on the way, or
    /// walking round to face the other way.
    fn moving(&self, route: &Route) -> bool {
        !self.still && (self.unload <= 0.0 || self.turning(route))
    }

    fn show(&self, route: &Route, out: &mut Vec<Seen>) {
        let pos = route.at(self.along);
        let facing = self.facing;
        let side = facing.perp();
        let moving = self.moving(route);
        let pace = self.pace();
        let piece = |piece, pos, rate, phase| Seen {
            piece,
            pos,
            facing,
            moving,
            rate,
            phase: self.phase + phase,
        };
        match self.load {
            Load::Cart => {
                out.push(piece(Piece::Cart, pos, 1.0, 0.0));
                // The drover walks by the ox's flank, stepping to match.
                let rate = pace / Kind::Farmer.pace().0;
                out.push(piece(
                    Piece::Drover,
                    pos + facing * 1.1 + side * 1.0,
                    rate,
                    1.3,
                ));
            }
            Load::Porters(n) => {
                for k in 0..n {
                    let wobble = if k % 2 == 0 { 0.3 } else { -0.3 };
                    let at = pos - facing * (1.6 * f32::from(k)) + side * wobble;
                    out.push(piece(
                        Piece::Porter,
                        at,
                        pace / Kind::Porter.pace().0,
                        0.9 * f32::from(k),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient::hamlets::tests::flat;

    const QUIET: Eyes<'static> = Eyes {
        armies: &[],
        fights: &[],
    };

    fn free(_: (i32, i32)) -> bool {
        false
    }

    fn flat_caravans() -> (Hamlets, Caravans) {
        let (map, _) = flat();
        let hamlets = Hamlets::new(&map);
        let caravans = Caravans::new(&map, &hamlets);
        (hamlets, caravans)
    }

    /// A caravan on the flat map, set on route 0 at `along`, going forward.
    fn set_at(along: f32) -> Caravans {
        let (_, mut c) = flat_caravans();
        let t = &mut c.trains[0];
        t.load = Load::Cart;
        t.along = along;
        t.forward = true;
        t.unload = 0.0;
        t.hold = 0.0;
        t.looked = 0.0;
        t.facing = c.routes[0].heading(true);
        c
    }

    /// Run train 0 on its own for `seconds`, in steps of a tenth.
    fn run(c: &mut Caravans, seconds: f32, eyes: Eyes) {
        for _ in 0..(seconds * 10.0).round() as usize {
            let route = c.routes[0];
            c.trains[0].step(0.1, &route, eyes, &free);
        }
    }

    #[test]
    fn the_cart_goes_at_the_pace_the_model_was_built_for() {
        // `oxcart.py`'s wheels turn once in its two-second Walk clip.
        assert!((CART_PACE - 1.19).abs() < 0.01, "{CART_PACE}");
    }

    #[test]
    fn neighbouring_hamlets_are_joined_but_not_through_a_third() {
        let (hamlets, c) = flat_caravans();
        // The village and the bazaar have the town between them: two routes,
        // not three, and each stops outside both its hamlets' models.
        assert_eq!(hamlets.len(), 3);
        assert_eq!(c.len(), 2);
        for r in &c.routes {
            for (end, hamlet) in [(r.from, 0..3), (r.to, 0..3)] {
                let nearest = hamlet
                    .map(|i| end.distance(hamlets.centre(i)) - hamlets.reach(i))
                    .fold(f32::MAX, f32::min);
                assert!(nearest >= PARK - 1e-3, "{r:?} ends {nearest} off");
            }
        }
    }

    #[test]
    fn shipped_lands_have_caravans_on_open_ground_only() {
        // Four Teeth has hamlets close enough with clear ground between them;
        // Ribcage Pass has two with none.
        let count = |choice: usize| {
            let map = crate::conquest::make_map(choice, 7);
            let hamlets = Hamlets::new(&map);
            let c = Caravans::new(&map, &hamlets);
            for r in &c.routes {
                assert!(r.len() >= SHORTEST * 0.5 && r.len() <= LONGEST, "{r:?}");
            }
            c.len()
        };
        assert_eq!(count(0), 0);
        assert!(count(1) >= 2);
        let steep = {
            let mut map = crate::conquest::make_map(1, 7);
            // Corrugate the ground so no way between hamlets is gentle.
            for (i, h) in map.samples.iter_mut().enumerate() {
                *h = h.saturating_add(if i % 2 == 0 { 6000 } else { 0 });
            }
            let hamlets = Hamlets::new(&map);
            Caravans::new(&map, &hamlets).len()
        };
        assert_eq!(steep, 0);
    }

    #[test]
    fn a_caravan_goes_to_the_far_end_waits_and_comes_back() {
        let mut c = set_at(0.0);
        let r = c.routes[0];
        let (mut far, mut back, mut waited) = (0.0f32, false, false);
        for _ in 0..((r.len() / CART_PACE * 3.0 + 40.0) * 10.0) as usize {
            c.trains[0].step(0.1, &r, QUIET, &free);
            let t = &c.trains[0];
            assert!(t.along >= -1e-3 && t.along <= r.len() + 1e-3);
            far = far.max(t.along);
            waited |= t.unload > 0.0;
            back |= far > r.len() - 0.5 && t.along < r.len() - 5.0;
        }
        assert!(far > r.len() - 0.5, "never got there: {far} of {}", r.len());
        assert!(waited && back);
    }

    #[test]
    fn the_cart_walks_round_while_it_turns_and_stands_when_it_has() {
        let mut c = set_at(0.0);
        let r = c.routes[0];
        c.trains[0].along = r.len() - 0.01;
        c.trains[0].facing = r.heading(true);
        run(&mut c, 0.5, QUIET);
        let wait = c.trains[0].unload;
        assert!(!c.trains[0].forward && wait > 7.0);
        // Still turning (a half turn takes pi / TURN_CART = 4.8 s), on its
        // feet while it does, then round and standing for the rest of the
        // wait.
        run(&mut c, 1.0, QUIET);
        let t = &c.trains[0];
        assert!(t.facing.dot(r.heading(false)) < 0.99);
        assert!(t.turning(&r) && t.moving(&r));
        run(&mut c, 5.0, QUIET);
        let t = &c.trains[0];
        assert!(t.unload > 0.5, "the wait ended before the turn was checked");
        assert!(t.facing.dot(r.heading(false)) > 0.99);
        assert!(!t.turning(&r) && !t.moving(&r));
    }

    #[test]
    fn an_army_stops_it_to_look_then_it_goes_on_and_a_fight_holds_it_until_quiet() {
        let mid = |c: &Caravans| c.routes[0].at(c.routes[0].len() / 2.0);
        // An army beside the road: it stands for LOOK seconds, then goes on
        // with the army still there.
        let mut c = set_at(set_at(0.0).routes[0].len() / 2.0);
        let (start, here) = (c.trains[0].along, mid(&c));
        let near = [here + Vec2::new(5.0, 0.0)];
        let army = Eyes {
            armies: &near,
            fights: &[],
        };
        run(&mut c, LOOK - 1.0, army);
        assert_eq!(c.trains[0].along, start);
        run(&mut c, 3.0, army);
        assert!(
            c.trains[0].along > start,
            "an army beside the road holds it for good"
        );
        // A fight at the far edge of the alarm holds it, however long it
        // goes on, and for CALM seconds after.
        let mut c = set_at(start);
        let fight = [here + Vec2::new(ALARM - 1.0, 0.0)];
        run(
            &mut c,
            20.0,
            Eyes {
                armies: &[],
                fights: &fight,
            },
        );
        assert_eq!(c.trains[0].along, start);
        run(&mut c, CALM - 1.0, QUIET);
        assert_eq!(c.trains[0].along, start);
        run(&mut c, 3.0, QUIET);
        assert!(c.trains[0].along > start);
        // A fight beyond the alarm does nothing.
        let mut c = set_at(start);
        let far = [here + Vec2::new(ALARM + 1.0, 0.0)];
        run(
            &mut c,
            3.0,
            Eyes {
                armies: &[],
                fights: &far,
            },
        );
        assert!(c.trains[0].along > start);
    }

    #[test]
    fn a_building_across_the_way_turns_it_back() {
        let r = set_at(0.0).routes[0];
        let along = r.len() * 0.4;
        let mut c = set_at(along);
        let ahead = r.at(along + BLOCK_LOOK);
        let wall = (ahead.x.floor() as i32, ahead.y.floor() as i32);
        let t = &mut c.trains[0];
        t.step(0.1, &r, QUIET, &|tile| tile == wall);
        assert!(!t.forward && t.unload > 0.0);
        assert_eq!(t.along, along);
        // Open ground ahead: no such thing.
        let mut c = set_at(along);
        c.trains[0].step(0.1, &r, QUIET, &free);
        assert!(c.trains[0].forward && c.trains[0].along > along);
    }

    #[test]
    fn only_caravans_near_the_camera_are_handed_over() {
        let mut c = set_at(10.0);
        c.advance(0.1, Vec2::splat(-1.0e6), 40.0, QUIET, free);
        assert!(c.seen().is_empty());
        let at = c.routes[0].at(c.trains[0].along);
        c.advance(0.1, at, 40.0, QUIET, free);
        assert!(!c.seen().is_empty());
        c.hush();
        assert!(c.seen().is_empty());
    }

    #[test]
    fn a_cart_has_its_drover_and_porters_walk_in_file() {
        let mut c = set_at(10.0);
        let r = c.routes[0];
        for load in [Load::Cart, Load::Porters(2)] {
            c.trains[0].load = load;
            let mut pieces = Vec::new();
            c.trains[0].show(&r, &mut pieces);
            let kinds: Vec<Piece> = pieces.iter().map(|s| s.piece).collect();
            match load {
                Load::Cart => assert_eq!(kinds, vec![Piece::Cart, Piece::Drover]),
                Load::Porters(_) => assert_eq!(kinds, vec![Piece::Porter, Piece::Porter]),
            }
            // Everyone faces the way the caravan goes.
            let f = pieces[0].facing;
            assert!(pieces.iter().all(|s| s.facing == f));
        }
    }

    #[test]
    fn the_same_map_gives_the_same_caravans() {
        let (map, _) = flat();
        let hamlets = Hamlets::new(&map);
        let (a, b) = (Caravans::new(&map, &hamlets), Caravans::new(&map, &hamlets));
        assert_eq!(a.routes, b.routes);
        let key = |c: &Caravans| {
            c.trains
                .iter()
                .map(|t| (t.load, t.along.to_bits(), t.forward))
                .collect::<Vec<_>>()
        };
        assert_eq!(key(&a), key(&b));
    }
}
