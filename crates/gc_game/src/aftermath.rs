//! What a battle leaves behind (docs/28-soldier-combat.md, "Aftermath and
//! wear"): the standard of a regiment wiped out, planted torn where it
//! fell, crows that come to the dead once the fighting has moved on, and
//! the grime a regiment picks up the longer it fights (cloth darkens a good
//! deal, skin and armour a little: the shader gives the tone byte only 8%).
//!
//! Only a picture. The simulation knows nothing of crows or grime; they
//! follow the wipe-outs it tells (`Event::Losses` for a regiment that is
//! gone) and the fights it shows (`RegimentView::engaged`), so nothing here
//! can change a match or a replay. The clock is the effects clock: still
//! while the game is paused or behind the menu.

use std::f32::consts::TAU;

use gc_render::BoxInstance;
use gc_sim::RegimentHandle;
use glam::{Vec2, Vec3};

use crate::land::smooth;
use crate::scenery::{self, Extra, Model};

/// Standards and flocks of crows kept at once, at most; when full the oldest
/// is forgotten.
const MAX_STANDARDS: usize = 96;
const MAX_FLOCKS: usize = 48;
const MAX_TROPHIES: usize = 96;
/// Captured standards one regiment carries at most (the oldest give way).
const TROPHIES_PER_REGIMENT: usize = 3;
/// Seconds a captured standard is carried before it is put away.
const TROPHY_SECONDS: f32 = 600.0;
/// How near a living regiment of a foe must stand to take the standard of
/// one wiped out, in tiles.
pub const TAKE_RADIUS: f32 = 10.0;
/// A captured standard rides this far behind the centre of the regiment
/// that took it, and this far apart from the others, in tiles.
const TROPHY_BEHIND: f32 = 1.6;
const TROPHY_APART: f32 = 0.9;
/// The fallen this close to where a regiment was wiped out are counted for
/// the crows.
pub const DEAD_RADIUS: f32 = 12.0;
/// A regiment wiped out this close to a flock that has not left yet joins it.
const MERGE_RADIUS: f32 = 7.0;
/// The standard stands this long, and shrinks away over the last
/// [`STANDARD_FADE`] seconds.
const STANDARD_SECONDS: f32 = 240.0;
const STANDARD_FADE: f32 = 8.0;
/// The first crow comes this long after the last regiment fell there, and
/// each of the others a little after the one before.
const CROW_ARRIVE: f32 = 14.0;
const CROW_GAP: f32 = 1.4;
/// A new crow circles this long (plus up to [`CIRCLE_EXTRA`] more) before
/// it settles, and all of them leave this long after the fall.
const CIRCLE_SECONDS: f32 = 7.0;
const CIRCLE_EXTRA: f32 = 6.0;
const CROW_STAY: f32 = 150.0;
/// The crows are gone this long after they started to leave.
const CROW_LEAVING: f32 = 7.0;
/// A flock is forgotten when its last crow has gone.
const FLOCK_SECONDS: f32 = CROW_STAY + 2.0 * MAX_CROWS as f32 + CROW_LEAVING;
/// Fallen soldiers per crow, and the most crows a flock draws.
const DEAD_PER_CROW: usize = 5;
const MIN_CROWS: usize = 2;
const MAX_CROWS: usize = 14;
/// Crows settle within this many tiles of the flock's centre.
const SPREAD: f32 = 3.5;
/// Soldiers this close to a flock flush its crows into the air.
const SCATTER_RADIUS: f32 = 5.0;
/// How fast the crows of a flock take to the air when disturbed (a
/// fraction of a flight a second) and come back to the ground.
const SCARE_RISE: f32 = 2.0;
const SCARE_FALL: f32 = 0.18;
/// Where a circling crow flies: radius, height and angular speed.
const CIRCLE_RADIUS: f32 = 4.5;
const CIRCLE_HEIGHT: f32 = 4.0;
const CIRCLE_SPEED: f32 = 0.7;

/// Seconds of melee that leave a regiment as grimy as it gets.
const GRIME_SECONDS: f32 = 120.0;
/// How dark the cloth of the filthiest soldiers is painted, against 1 as
/// painted. Their tone byte goes all the way down, which is as much as the
/// shader will do with it (8% darker, a little cooler).
const GRIME_DARKEST: f32 = 0.72;
/// How grimy the dead are, whichever regiment they fought in.
pub const DEAD_DIRT: f32 = 0.6;

const CROW_COLOR: [u8; 4] = [22, 22, 28, 0];
/// A crow's body: width, height and length in tiles.
const CROW_SIZE: [f32; 3] = [0.13, 0.12, 0.26];
/// A crow's span with its wings out.
const WING_SPAN: f32 = 0.55;

/// A torn standard planted where a regiment was wiped out.
struct Standard {
    at: Vec2,
    /// The way the regiment faced, which the standard follows.
    facing: Vec2,
    /// Its team colour.
    tint: [u8; 3],
    /// Seconds since it fell.
    age: f32,
}

/// A standard taken from a wiped-out regiment, carried in the rear by the
/// regiment that did it.
struct Trophy {
    winner: RegimentHandle,
    /// The colour of the regiment it was taken from.
    tint: [u8; 3],
    /// Seconds since it was taken.
    age: f32,
    /// Counts the trophies taken, so equal ages still have an order.
    seq: u32,
}

/// The crows over a battlefield.
struct Flock {
    at: Vec2,
    /// Fallen soldiers around the spot when the last regiment fell.
    dead: usize,
    /// Seconds since the first regiment fell there.
    age: f32,
    /// 0 with the crows on the ground, 1 with them all in the air.
    scare: f32,
    seed: u32,
}

/// What is known of a regiment on the field.
#[derive(Clone, Copy)]
struct Seen {
    handle: RegimentHandle,
    /// Seconds in melee, up to [`GRIME_SECONDS`].
    fought: f32,
    worker: bool,
}

#[derive(Default)]
pub struct Aftermath {
    standards: Vec<Standard>,
    trophies: Vec<Trophy>,
    /// Trophies taken so far (see [`Trophy::seq`]).
    taken: u32,
    flocks: Vec<Flock>,
    /// Per regiment slot.
    seen: Vec<Option<Seen>>,
    /// Scratch for [`Aftermath::advance`].
    living: Vec<Vec2>,
}

impl Aftermath {
    /// Forget it all: a new match, a seek.
    pub fn clear(&mut self) {
        self.standards.clear();
        self.trophies.clear();
        self.flocks.clear();
        self.seen.clear();
    }

    /// Regiment `h` is on the field for `dt` more seconds, `engaged` in
    /// melee or not.
    pub fn tend(&mut self, h: RegimentHandle, engaged: bool, worker: bool, dt: f32) {
        let i = h.index as usize;
        if self.seen.len() <= i {
            self.seen.resize(i + 1, None);
        }
        let mut seen = self.slot(h).unwrap_or(Seen {
            handle: h,
            fought: 0.0,
            worker,
        });
        if engaged {
            seen.fought = (seen.fought + dt).min(GRIME_SECONDS);
        }
        self.seen[i] = Some(seen);
    }

    /// Regiment `h` of `tint`'s colour was wiped out at `at`, facing
    /// `facing`, with `dead` fallen soldiers lying around (its own included).
    /// A `victor` standing by takes its standard and carries it; else the
    /// torn standard is left where it fell. Workers go unmourned.
    pub fn wiped(
        &mut self,
        h: RegimentHandle,
        at: Vec2,
        facing: Vec2,
        tint: [u8; 3],
        dead: usize,
        victor: Option<RegimentHandle>,
    ) {
        if self.slot(h).is_some_and(|s| s.worker) {
            return;
        }
        if let Some(winner) = victor {
            self.take(winner, tint);
        } else {
            push_oldest(
                &mut self.standards,
                MAX_STANDARDS,
                Standard {
                    at,
                    facing,
                    tint,
                    age: 0.0,
                },
                |s| s.age,
            );
        }
        let near = |f: &&mut Flock| f.age < CROW_STAY && f.at.distance(at) <= MERGE_RADIUS;
        if let Some(f) = self.flocks.iter_mut().find(near) {
            // The same fight going on: more crows for the dead, and if none
            // have come yet, a fresh wait.
            f.dead = dead;
            if f.age < CROW_ARRIVE {
                f.age = 0.0;
            }
            return;
        }
        let flock = Flock {
            at,
            dead,
            age: 0.0,
            scare: 0.0,
            seed: at.x.to_bits().wrapping_mul(0x9E37_79B9) ^ at.y.to_bits(),
        };
        push_oldest(&mut self.flocks, MAX_FLOCKS, flock, |f| f.age);
    }

    /// Regiment `winner` takes a standard of `tint`'s colour; past three, its
    /// oldest is put away.
    fn take(&mut self, winner: RegimentHandle, tint: [u8; 3]) {
        let held = || {
            self.trophies
                .iter()
                .enumerate()
                .filter(|(_, t)| t.winner == winner)
        };
        if held().count() >= TROPHIES_PER_REGIMENT
            && let Some((i, _)) = held().min_by_key(|(_, t)| t.seq)
        {
            self.trophies.remove(i);
        }
        self.taken = self.taken.wrapping_add(1);
        push_oldest(
            &mut self.trophies,
            MAX_TROPHIES,
            Trophy {
                winner,
                tint,
                age: 0.0,
                seq: self.taken,
            },
            |t| t.age,
        );
    }

    /// `dt` seconds on: standards and flocks age and expire, and crows near
    /// soldiers (`living`: where they stand) take to the air.
    pub fn advance(&mut self, dt: f32, living: impl Iterator<Item = Vec2>) {
        if self.standards.is_empty() && self.flocks.is_empty() && self.trophies.is_empty() {
            return;
        }
        self.living.clear();
        self.living.extend(living);
        for s in &mut self.standards {
            s.age += dt;
        }
        self.standards.retain(|s| s.age < STANDARD_SECONDS);
        for t in &mut self.trophies {
            t.age += dt;
        }
        self.trophies.retain(|t| t.age < TROPHY_SECONDS);
        for f in &mut self.flocks {
            f.age += dt;
            let disturbed = (self.living.iter())
                .any(|p| p.distance_squared(f.at) <= SCATTER_RADIUS * SCATTER_RADIUS);
            f.scare = if disturbed {
                (f.scare + SCARE_RISE * dt).min(1.0)
            } else {
                (f.scare - SCARE_FALL * dt).max(0.0)
            };
        }
        self.flocks.retain(|f| f.age < FLOCK_SECONDS);
    }

    /// The torn standards planted where regiments fell.
    pub fn standards<'a>(
        &'a self,
        ground: impl Fn(Vec2) -> f32 + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        self.standards.iter().map(move |s| Extra {
            model: Model::BannerShattered,
            at: Vec3::new(s.at.x, ground(s.at), s.at.y),
            turn: scenery::turn_of(s.facing),
            scale: ((STANDARD_SECONDS - s.age) / STANDARD_FADE).clamp(0.0, 1.0),
            tint: s.tint.map(|c| c / 3),
            phase: scenery::turn_of(s.facing),
            lean: 0,
            fall: 0,
        })
    }

    /// The standards regiments carry off, as `pose` places each carrier
    /// (its centre and facing, or `None` when it is gone or out of sight):
    /// planted in the rear rank, side by side.
    pub fn trophies<'a>(
        &'a self,
        pose: impl Fn(RegimentHandle) -> Option<(Vec2, Vec2)> + 'a,
        ground: impl Fn(Vec2) -> f32 + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        self.trophies.iter().filter_map(move |t| {
            let (centre, facing) = pose(t.winner)?;
            // Where it stands among the carrier's others, oldest to the left.
            let mine = || self.trophies.iter().filter(|o| o.winner == t.winner);
            let rank = mine().filter(|o| o.seq < t.seq).count();
            let side = (rank as f32 - (mine().count() as f32 - 1.0) / 2.0) * TROPHY_APART;
            let at = centre - facing * TROPHY_BEHIND + facing.perp() * side;
            let turn = scenery::turn_of(facing);
            Some(Extra {
                model: Model::BannerShattered,
                at: Vec3::new(at.x, ground(at), at.y),
                turn,
                scale: 0.8 * ((TROPHY_SECONDS - t.age) / STANDARD_FADE).clamp(0.0, 1.0),
                tint: t.tint.map(|c| c / 2),
                phase: turn,
                lean: 0,
                fall: 0,
            })
        })
    }

    /// The crows over each flock `sees` (centre and radius), as boxes in
    /// `out`, leaving `room` boxes free.
    pub fn crows(
        &self,
        ground: impl Fn(Vec2) -> f32,
        sees: impl Fn(Vec3, f32) -> bool,
        out: &mut Vec<BoxInstance>,
        room: usize,
    ) {
        for f in &self.flocks {
            let mid = Vec3::new(f.at.x, ground(f.at), f.at.y);
            if !sees(mid + Vec3::Y * CIRCLE_HEIGHT, SPREAD + CIRCLE_RADIUS * 2.0) {
                continue;
            }
            for k in 0..crows_of(f.dead) {
                if out.len() >= room {
                    return;
                }
                if let Some(b) = crow(f, k, mid.y, &ground) {
                    out.push(b);
                }
            }
        }
    }

    /// The same crows as [`Aftermath::crows`], as birds for the crow model
    /// to draw, leaving `room` free in `out`.
    pub fn birds(
        &self,
        ground: impl Fn(Vec2) -> f32,
        sees: impl Fn(Vec3, f32) -> bool,
        out: &mut Vec<Bird>,
        room: usize,
    ) {
        for f in &self.flocks {
            let mid = Vec3::new(f.at.x, ground(f.at), f.at.y);
            if !sees(mid + Vec3::Y * CIRCLE_HEIGHT, SPREAD + CIRCLE_RADIUS * 2.0) {
                continue;
            }
            for k in 0..crows_of(f.dead) {
                if out.len() >= room {
                    return;
                }
                out.extend(bird(f, k, mid.y, &ground));
            }
        }
    }

    /// How dirty regiment `h`'s soldiers are, 0 (as painted) to 1.
    pub fn dirt(&self, h: RegimentHandle) -> f32 {
        self.slot(h).map_or(0.0, |s| s.fought / GRIME_SECONDS)
    }

    fn slot(&self, h: RegimentHandle) -> Option<Seen> {
        self.seen
            .get(h.index as usize)
            .copied()
            .flatten()
            .filter(|s| s.handle == h)
    }
}

/// Add `item`, replacing the oldest if `list` is full.
fn push_oldest<T>(list: &mut Vec<T>, max: usize, item: T, age: impl Fn(&T) -> f32) {
    if list.len() < max {
        list.push(item);
    } else if let Some(oldest) = list.iter_mut().max_by(|a, b| age(a).total_cmp(&age(b))) {
        *oldest = item;
    }
}

/// A soldier's team colour and tone byte (`models::Look::tone`) as `dirt`
/// (0 to 1, see [`Aftermath::dirt`]) leaves them.
pub fn worn(color: [u8; 3], tone: u8, dirt: f32) -> ([u8; 3], u8) {
    let shade = 1.0 - (1.0 - GRIME_DARKEST) * dirt;
    (
        color.map(|c| (f32::from(c) * shade).round() as u8),
        (f32::from(tone) * (1.0 - dirt)).round() as u8,
    )
}

fn crows_of(dead: usize) -> usize {
    (dead / DEAD_PER_CROW).clamp(MIN_CROWS, MAX_CROWS)
}

/// Crow `k` of flock `s` (whose centre lies at height `floor`), if it is
/// there: out of sight before it comes and after it leaves, circling as it
/// arrives, on the ground pecking among the dead, and in the air again
/// whenever soldiers walk near.
fn crow(s: &Flock, k: usize, floor: f32, ground: &impl Fn(Vec2) -> f32) -> Option<BoxInstance> {
    let b = bird(s, k, floor, ground)?;
    let wings = WING_SPAN * (0.35 + 0.65 * b.flapping);
    Some(BoxInstance {
        pos: b.pos.to_array(),
        facing: b.facing.to_array(),
        size: [
            CROW_SIZE[0] + (wings - CROW_SIZE[0]) * b.aloft,
            CROW_SIZE[1] * (1.0 - 0.3 * (1.0 - b.aloft) * b.peck.max(0.0)),
            CROW_SIZE[2],
        ],
        color: CROW_COLOR,
        lift: b.lift,
    })
}

/// One crow of a flock as the picture draws it: where it is and which way it
/// faces, its height above the ground, how far aloft it is (0 to 1), and
/// the beat of its wings and its peck.
#[derive(Clone, Copy, Debug)]
pub struct Bird {
    pub pos: Vec2,
    pub facing: Vec2,
    /// Tiles above the ground.
    pub lift: f32,
    /// 0 on the ground, 1 in the air.
    pub aloft: f32,
    /// A seed for where in its clip the crow is, so they do not move as one.
    pub phase: f32,
    flapping: f32,
    peck: f32,
}

fn bird(s: &Flock, k: usize, floor: f32, ground: &impl Fn(Vec2) -> f32) -> Option<Bird> {
    let h = |axis| 0.5 + 0.5 * crate::spread_hash(s.seed, k, axis);
    let comes = CROW_ARRIVE + CROW_GAP * k as f32 * (0.6 + 0.8 * h(1));
    let t = s.age - comes;
    if t < 0.0 {
        return None;
    }
    let leaves = CROW_STAY + 2.0 * k as f32;
    let gone = s.age - leaves;
    if gone > CROW_LEAVING {
        return None;
    }
    // Aloft: circling on arrival and whenever scared, then climbing away.
    let circling = CIRCLE_SECONDS + CIRCLE_EXTRA * h(2);
    let aloft = if gone > 0.0 {
        1.0
    } else {
        (1.0 - smooth(circling, circling + 2.0, t)).max(s.scare)
    };
    let away = gone.max(0.0);
    let spot = s.at + Vec2::from_angle(TAU * h(3)) * (SPREAD * h(4).sqrt());
    let theta = TAU * h(5) + s.age * CIRCLE_SPEED * (0.8 + 0.4 * h(6));
    let radius = CIRCLE_RADIUS * (0.7 + 0.6 * h(7)) + away * 5.0;
    let ring = s.at + Vec2::from_angle(theta) * radius;
    let pos = spot.lerp(ring, aloft);
    let up = CIRCLE_HEIGHT * (0.8 + 0.5 * h(8)) + away * 4.0;
    let flapping = (s.age * (8.0 + 3.0 * h(9)) + TAU * h(10)).sin().abs();
    // On the ground he pecks and now and then hops; in the air the wings beat.
    let peck = (s.age * (1.5 + h(11)) + TAU * h(12)).sin();
    let hop = (s.age * 0.35 + TAU * h(13)).sin().max(0.97) - 0.97;
    let facing = if aloft > 0.5 {
        Vec2::from_angle(theta + TAU / 4.0)
    } else {
        Vec2::from_angle(TAU * h(14) + 0.4 * peck)
    };
    Some(Bird {
        pos,
        facing,
        lift: aloft * (up + floor - ground(pos)) + hop * 12.0,
        aloft,
        phase: h(15) * 3.0,
        flapping,
        peck,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: Vec2) -> f32 {
        0.0
    }

    fn handle(index: u32) -> RegimentHandle {
        RegimentHandle {
            index,
            generation: 0,
        }
    }

    fn crows_at(a: &Aftermath) -> Vec<BoxInstance> {
        let mut out = Vec::new();
        a.crows(flat, |_, _| true, &mut out, usize::MAX);
        out
    }

    fn after_a_wipe_out() -> Aftermath {
        let mut a = Aftermath::default();
        a.wiped(
            handle(1),
            Vec2::new(30.0, 30.0),
            Vec2::X,
            [200, 40, 40],
            40,
            None,
        );
        a
    }

    #[test]
    fn a_fallen_standard_is_planted_dimmed_and_gone_in_time() {
        let mut a = after_a_wipe_out();
        let stood: Vec<_> = a.standards(flat).collect();
        assert_eq!(stood.len(), 1);
        assert_eq!(stood[0].model, Model::BannerShattered);
        assert_eq!(stood[0].tint, [66, 13, 13]);
        assert_eq!(stood[0].scale, 1.0);
        a.advance(STANDARD_SECONDS - STANDARD_FADE / 2.0, std::iter::empty());
        let fading: Vec<_> = a.standards(flat).collect();
        assert!(fading[0].scale > 0.0 && fading[0].scale < 1.0);
        a.advance(STANDARD_FADE, std::iter::empty());
        assert_eq!(a.standards(flat).count(), 0);
    }

    #[test]
    fn a_victor_by_takes_the_standard_and_carries_it_in_the_rear() {
        let mut a = Aftermath::default();
        let fell = |a: &mut Aftermath, n: u32, tint: [u8; 3]| {
            let (at, facing) = (Vec2::new(30.0, 30.0), Vec2::X);
            a.wiped(handle(n), at, facing, tint, 8, Some(handle(9)));
        };
        let pose = |h: RegimentHandle| (h == handle(9)).then_some((Vec2::new(50.0, 40.0), Vec2::X));
        fell(&mut a, 1, [200, 40, 40]);
        assert_eq!(a.standards(flat).count(), 0, "nothing is left on the field");
        let carried: Vec<_> = a.trophies(pose, flat).collect();
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].model, Model::BannerShattered);
        assert!(carried[0].at.x < 50.0, "behind the centre");
        assert_eq!(carried[0].at.z, 40.0, "and in the middle of the rank");
        // A carrier that is gone, unseen or another one shows nothing.
        assert_eq!(a.trophies(|_| None, flat).count(), 0);
        assert_eq!(
            a.trophies(|h| (h == handle(8)).then_some((Vec2::ZERO, Vec2::X)), flat)
                .count(),
            0
        );
        // At most three, side by side, the oldest to the left; the first is
        // put away for the fourth, though all were taken in the same tick.
        for (n, red) in [(2, 100), (3, 150), (4, 250)] {
            fell(&mut a, n, [red, 0, 0]);
        }
        let mut row: Vec<_> = a.trophies(pose, flat).collect();
        assert_eq!(row.len(), TROPHIES_PER_REGIMENT);
        row.sort_by(|p, q| p.at.z.total_cmp(&q.at.z));
        let reds: Vec<u8> = row.iter().map(|e| e.tint[0]).collect();
        assert_eq!(
            reds,
            [50, 75, 125],
            "the first (200) is gone, the rest in order"
        );
        // And in the end put away.
        a.advance(TROPHY_SECONDS + 1.0, std::iter::empty());
        assert_eq!(a.trophies(pose, flat).count(), 0);
    }

    #[test]
    fn crows_come_after_the_fighting_and_leave_in_the_end() {
        let mut a = after_a_wipe_out();
        assert!(crows_at(&a).is_empty(), "none while it is still going on");
        a.advance(CROW_ARRIVE + 3.0, std::iter::empty());
        assert!(!crows_at(&a).is_empty(), "the first crows have come");
        a.advance(60.0, std::iter::empty());
        assert_eq!(crows_at(&a).len(), crows_of(40), "all of them by now");
        a.advance(CROW_STAY + 20.0 - 63.0, std::iter::empty());
        assert!(crows_at(&a).is_empty(), "and gone again");
    }

    #[test]
    fn the_crow_model_draws_the_crows_the_boxes_would() {
        let mut a = after_a_wipe_out();
        // Circling as they come, settled on the field, and flushed up again.
        let mut aloft = false;
        for (age, near) in [(CROW_ARRIVE + 3.0, false), (60.0, false), (0.0, true)] {
            let scare: Vec<Vec2> = if near {
                vec![Vec2::new(31.0, 30.0)]
            } else {
                vec![]
            };
            for _ in 0..if near { 10 } else { 1 } {
                a.advance(if near { 0.2 } else { age }, scare.iter().copied());
            }
            let mut birds = Vec::new();
            a.birds(flat, |_, _| true, &mut birds, usize::MAX);
            let boxes = crows_at(&a);
            assert_eq!(birds.len(), boxes.len());
            assert!(!birds.is_empty());
            for (b, x) in birds.iter().zip(&boxes) {
                assert_eq!(b.pos.to_array(), x.pos);
                assert_eq!(b.facing.to_array(), x.facing);
                assert_eq!(b.lift, x.lift);
                assert!((0.0..=1.0).contains(&b.aloft));
                aloft |= b.aloft > 0.9 && b.lift > 1.0;
            }
        }
        assert!(aloft, "no crow was ever up in the air");
        let mut a = after_a_wipe_out();
        a.advance(CROW_ARRIVE + 60.0, std::iter::empty());
        let mut birds = Vec::new();
        a.birds(flat, |_, _| true, &mut birds, usize::MAX);
        assert_eq!(birds.len(), crows_of(40));
        let mut few = Vec::new();
        a.birds(flat, |_, _| true, &mut few, 3);
        assert_eq!(few.len(), 3);
        let mut none = Vec::new();
        a.birds(flat, |_, _| false, &mut none, usize::MAX);
        assert!(none.is_empty());
    }

    #[test]
    fn crows_settle_on_the_ground_and_rise_when_soldiers_come() {
        let mut a = after_a_wipe_out();
        a.advance(CROW_ARRIVE + 60.0, std::iter::empty());
        assert!(
            crows_at(&a).iter().all(|c| c.lift < 0.5),
            "settled on the field"
        );
        let near = [Vec2::new(31.0, 30.0)];
        for _ in 0..10 {
            a.advance(0.2, near.iter().copied());
        }
        assert!(
            crows_at(&a).iter().all(|c| c.lift > 1.0),
            "a regiment walking up flushes them"
        );
        for _ in 0..100 {
            a.advance(0.2, std::iter::empty());
        }
        assert!(
            crows_at(&a).iter().all(|c| c.lift < 0.5),
            "and they settle again once it has passed"
        );
    }

    #[test]
    fn crows_fly_at_a_height_over_the_ground_below_them() {
        let mut a = after_a_wipe_out();
        a.advance(CROW_ARRIVE + 3.0, std::iter::empty());
        let (mut hill, mut dip) = (Vec::new(), Vec::new());
        for _ in 0..10 {
            a.advance(0.3, [Vec2::new(31.0, 30.0)].into_iter());
        }
        // The ground is level with the crows' flight when drawn at its own
        // height plus the lift: the same for a slope as for a plain.
        let on = |ground: fn(Vec2) -> f32, out: &mut Vec<f32>| {
            let mut boxes = Vec::new();
            a.crows(ground, |_, _| true, &mut boxes, usize::MAX);
            out.extend(boxes.iter().map(|b| ground(Vec2::from(b.pos)) + b.lift));
        };
        on(|p| 0.3 * p.x, &mut hill);
        on(|p| -0.3 * p.x, &mut dip);
        assert!(!hill.is_empty() && hill.len() == dip.len());
        let floor = |slope: f32| slope * 30.0;
        assert!(
            hill.iter()
                .all(|y| (y - floor(0.3)).abs() < 8.0 && *y > floor(0.3))
        );
        assert!(
            dip.iter()
                .all(|y| (y - floor(-0.3)).abs() < 8.0 && *y > floor(-0.3))
        );
    }

    #[test]
    fn every_wipe_out_plants_a_standard_but_nearby_ones_share_the_crows() {
        let mut a = after_a_wipe_out();
        a.advance(CROW_ARRIVE + 30.0, std::iter::empty());
        let before = crows_at(&a).len();
        a.wiped(
            handle(2),
            Vec2::new(33.0, 31.0),
            Vec2::Y,
            [40, 40, 200],
            60,
            None,
        );
        assert_eq!(
            a.standards(flat).count(),
            2,
            "each where its regiment stood"
        );
        let tints: Vec<_> = a.standards(flat).map(|s| s.tint).collect();
        assert_ne!(tints[0], tints[1]);
        assert_eq!(a.flocks.len(), 1);
        assert!(crows_at(&a).len() >= before, "no crow vanishes");
        a.wiped(
            handle(3),
            Vec2::new(90.0, 90.0),
            Vec2::Y,
            [40, 40, 200],
            10,
            None,
        );
        assert_eq!(a.flocks.len(), 2);
    }

    #[test]
    fn a_flock_that_has_left_is_not_joined() {
        let mut a = after_a_wipe_out();
        a.advance(CROW_STAY + 1.0, std::iter::empty());
        a.wiped(
            handle(2),
            Vec2::new(31.0, 30.0),
            Vec2::Y,
            [40, 40, 200],
            60,
            None,
        );
        assert_eq!(a.flocks.len(), 2, "a new battle there has crows of its own");
    }

    #[test]
    fn workers_are_not_mourned() {
        let mut a = Aftermath::default();
        a.tend(handle(5), true, true, 1.0);
        a.wiped(handle(5), Vec2::new(30.0, 30.0), Vec2::X, [90; 3], 40, None);
        assert_eq!(a.standards(flat).count(), 0);
        assert!(a.flocks.is_empty());
    }

    #[test]
    fn the_oldest_is_forgotten_when_full_and_crows_keep_to_the_room_given() {
        let mut a = Aftermath::default();
        for i in 0..MAX_FLOCKS + 5 {
            a.wiped(
                handle(1),
                Vec2::new(10.0 * i as f32, 0.0),
                Vec2::X,
                [90; 3],
                50,
                None,
            );
            a.advance(1.0, std::iter::empty());
        }
        assert_eq!(a.flocks.len(), MAX_FLOCKS);
        let at = |x: f32| a.flocks.iter().any(|f| f.at.x == x);
        assert!(!at(0.0) && !at(40.0), "the first five are gone");
        assert!(
            at(10.0 * (MAX_FLOCKS + 4) as f32),
            "and the newest is there"
        );
        a.advance(CROW_ARRIVE + 100.0, std::iter::empty());
        let mut out = Vec::new();
        a.crows(flat, |_, _| true, &mut out, 20);
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn crows_out_of_view_are_not_made() {
        let mut a = after_a_wipe_out();
        a.advance(CROW_ARRIVE + 60.0, std::iter::empty());
        let mut out = Vec::new();
        a.crows(flat, |_, _| false, &mut out, usize::MAX);
        assert!(out.is_empty());
    }

    #[test]
    fn grime_builds_with_fighting_and_belongs_to_one_regiment() {
        let mut a = Aftermath::default();
        let (r, other) = (handle(3), handle(4));
        a.tend(r, false, false, 50.0);
        assert_eq!(a.dirt(r), 0.0, "marching is clean");
        a.tend(r, true, false, GRIME_SECONDS / 2.0);
        assert!((a.dirt(r) - 0.5).abs() < 1e-6);
        a.tend(r, true, false, GRIME_SECONDS * 3.0);
        assert_eq!(a.dirt(r), 1.0, "it only gets so dirty");
        assert_eq!(a.dirt(other), 0.0);
        // A new regiment in the slot starts clean.
        let reused = RegimentHandle {
            index: 3,
            generation: 1,
        };
        assert_eq!(a.dirt(reused), 0.0);
        a.tend(reused, true, false, 1.0);
        assert!(a.dirt(reused) > 0.0 && a.dirt(reused) < 0.1);
    }

    #[test]
    fn wear_darkens_the_cloth_a_good_deal_and_the_tone_to_its_floor() {
        assert_eq!(worn([200, 100, 40], 128, 0.0), ([200, 100, 40], 128));
        let ([r, g, b], tone) = worn([200, 100, 40], 128, 1.0);
        assert_eq!([r, g, b], [144, 72, 29]);
        assert_eq!(tone, 0);
    }
}
