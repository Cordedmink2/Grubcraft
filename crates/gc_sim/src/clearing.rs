//! Clearing land (Conquest, idea 22, docs/32-changing-land.md): workers
//! fell woods and grub out scrub and thicket so a base can grow.
//!
//! A worker ordered to clear an area ([`crate::Command::Clear`]) works its
//! overgrown tiles ([`Growth`]) one at a time, the nearest first: it walks
//! up to the tile, works it for its growth's time ([`clear_ticks`], at the
//! match's tempo), and the tile becomes open ground. Several workers on the
//! same tile share its work. A worker ordered to build on overgrown ground
//! clears the footprint first, having paid; then it steps off the footprint
//! and lays the foundation. Felled woods and thicket pay a little Grub
//! ([`timber`]).
//!
//! This module only decides which tiles are cleared in a tick
//! ([`Clearing::cleared`]); the world changes the ground
//! (`World::change_ground`).

use gc_fixed::{Fx, FxVec2};

use crate::regiments::{Order, Regiments};
use crate::structures::{Approach, BuildingId, RETRY_TICKS, STALLS, footprint_centre};
use crate::terrain::{Growth, Terrain};
use crate::{PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Longest side of an area one Clear order takes, in tiles.
pub const MAX_CLEAR_SIDE: i32 = 16;

/// Ticks one worker regiment takes to clear a tile of `growth`, at
/// Standard tempo.
pub const fn clear_ticks(growth: Growth) -> u32 {
    match growth {
        Growth::Open => 0,
        Growth::Scrub => 3 * TICKS_PER_SECOND,
        Growth::Woods => 8 * TICKS_PER_SECOND,
        Growth::Thicket => 15 * TICKS_PER_SECOND,
    }
}

/// Grub the timber from a cleared tile of `growth` pays.
pub const fn timber(growth: Growth) -> u32 {
    match growth {
        Growth::Open | Growth::Scrub => 0,
        Growth::Woods => 3,
        Growth::Thicket => 5,
    }
}

/// Tiles a worker may give up on in one order before it stops.
const SKIPS: usize = 4;
/// Work a worker does a tick, in hundredths of a tick of clearing time.
const HAND: u32 = 100;

/// What a worker is clearing (see `World::clearing_of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClearingView {
    /// The area: north-west tile, width and depth, in tiles.
    pub area: [i32; 4],
    /// The building (type, north-west tile and quarter turns) it has paid for and will
    /// lay on the area once it is cleared, if any.
    pub then: Option<(BuildingId, (i32, i32), u8)>,
}

/// A building paid for, to be laid once its footprint is cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pledge {
    pub(crate) owner: PlayerId,
    pub(crate) building: BuildingId,
    pub(crate) tile: (i32, i32),
    pub(crate) turn: u8,
    pub(crate) grub: u32,
    pub(crate) marrow: u32,
}

/// One worker's clearing order.
#[derive(Clone, Copy, Debug)]
struct Job {
    /// The area: north-west tile, width and depth.
    area: [i32; 4],
    /// The building to lay once it is cleared (its footprint is the area).
    then: Option<Pledge>,
    /// The tile it is walking to or working (row by row).
    at: Option<u32>,
    /// Ticks until its next try to reach the tile.
    clock: u16,
    /// How far it was from the tile when it last set off for it, and how
    /// many times in a row that brought it no nearer.
    gap: Fx,
    stalls: u8,
    /// Tiles it gave up on, `skipped` of them.
    skip: [u32; SKIPS],
    skipped: u8,
    /// The generation of the regiment the order was given to.
    generation: u16,
}

impl Job {
    /// On to the next tile: the walk to it starts afresh.
    fn next(&mut self) {
        (self.at, self.gap, self.stalls) = (None, Fx::MAX, 0);
    }
}

/// What a tick of clearing came to, for the world to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Done {
    /// The tile `(x, y)` is cleared, by a worker of `owner`'s.
    Tile { x: i32, y: i32, owner: PlayerId },
    /// Worker slot `r` cleared and left the footprint of `pledge`: lay it.
    Lay { r: usize, pledge: Pledge },
    /// Worker slot `r` (or the regiment that was in it) gave up on, or was
    /// lost with, `pledge`: its price goes back.
    Refund { pledge: Pledge },
}

/// Every worker's clearing order, by regiment slot.
pub(crate) struct Clearing {
    job: alloc::vec::Vec<Option<Job>>,
    /// Tiles part cleared: the tile (row by row) and work left in
    /// hundredths of a tick, in tile order.
    worked: alloc::vec::Vec<(u32, u32)>,
    /// Scratch: the tiles workers are at, sorted (kept for its room).
    at: alloc::vec::Vec<u32>,
    /// What the last tick came to (see [`Done`]).
    pub(crate) done: alloc::vec::Vec<Done>,
    /// Tempo: clearing takes this percent of its Standard time.
    percent: u32,
}

impl Clearing {
    pub(crate) fn new(regiments: usize, build_percent: u32) -> Clearing {
        Clearing {
            job: alloc::vec![None; regiments],
            worked: alloc::vec::Vec::with_capacity(regiments),
            at: alloc::vec::Vec::with_capacity(regiments),
            // One tile and one footprint per worker at most, and each
            // order dropped.
            done: alloc::vec::Vec::with_capacity(2 * regiments),
            percent: build_percent,
        }
    }

    /// Worker slot `r` clears `area` (its tiles on the map), then lays
    /// `then` if given.
    pub(crate) fn start(
        &mut self,
        regs: &mut Regiments,
        r: usize,
        area: [i32; 4],
        then: Option<Pledge>,
    ) {
        self.job[r] = Some(Job {
            area,
            then,
            at: None,
            clock: 0,
            gap: Fx::MAX,
            stalls: 0,
            skip: [0; SKIPS],
            skipped: 0,
            generation: regs.generation[r],
        });
        regs.target[r] = None;
    }

    /// Worker slot `r` was given another order: it stops clearing.
    /// Returns the building it had paid for and not laid, if any.
    pub(crate) fn stop(&mut self, r: usize) -> Option<Pledge> {
        self.job.get_mut(r)?.take()?.then
    }

    /// The area worker slot `r` is clearing, and the building it will lay
    /// there, if it is clearing.
    pub(crate) fn job_of(&self, regs: &Regiments, r: usize) -> Option<([i32; 4], Option<Pledge>)> {
        self.live(regs, r).map(|job| (job.area, job.then))
    }

    /// Worker slot `r`'s order, if the regiment it was given to is still
    /// there.
    fn live(&self, regs: &Regiments, r: usize) -> Option<&Job> {
        let job = self.job.get(r)?.as_ref()?;
        (regs.alive[r] && regs.generation[r] == job.generation).then_some(job)
    }

    /// Every building paid for and waiting on its footprint being cleared.
    pub(crate) fn pledges(&self) -> impl Iterator<Item = Pledge> + '_ {
        self.job.iter().flatten().filter_map(|j| j.then)
    }

    /// One tick of clearing: workers walk to their tiles and work them.
    /// What it came to is left in [`Clearing::done`].
    pub(crate) fn update(&mut self, regs: &mut Regiments, units: &[UnitStats], terrain: &Terrain) {
        self.done.clear();
        for r in 0..self.job.len() {
            let Some(mut job) = self.job[r] else {
                continue;
            };
            if self.live(regs, r).is_none() || regs.morale_state[r].broken() {
                self.drop(r, job);
                continue;
            }
            // Fighting comes first.
            if regs.engaged[r].is_some() {
                continue;
            }
            job.clock = job.clock.saturating_sub(regs.steps(r));
            let kept = self.work(regs, units, terrain, r, &mut job);
            self.job[r] = kept.then_some(job);
        }
        // Work on a tile nobody is at any more is forgotten.
        if !self.worked.is_empty() {
            let at = &mut self.at;
            at.clear();
            at.extend(self.job.iter().flatten().filter_map(|j| j.at));
            at.sort_unstable();
            self.worked.retain(|&(t, _)| at.binary_search(&t).is_ok());
        }
    }

    /// An order ends without its building laid: its price goes back.
    fn drop(&mut self, r: usize, job: Job) {
        self.job[r] = None;
        if let Some(pledge) = job.then {
            self.done.push(Done::Refund { pledge });
        }
    }

    /// Worker slot `r`'s tick on `job`. Returns whether the order goes on.
    fn work(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
        r: usize,
        job: &mut Job,
    ) -> bool {
        let side = terrain.growth_side();
        let left = |t: u32| {
            let (x, y) = ((t % side) as i32, (t / side) as i32);
            let cleared =
                |d: &Done| matches!(*d, Done::Tile { x: dx, y: dy, .. } if (dx, dy) == (x, y));
            terrain.growth(x, y) != Growth::Open && !self.done.iter().any(cleared)
        };
        if job.at.is_some_and(|t| !left(t)) {
            job.next();
        }
        if job.at.is_none() {
            job.at = self.nearest(regs.position[r], terrain, job, &left);
        }
        let unit = &units[regs.unit[r] as usize];
        let Some(t) = job.at else {
            // All cleared: lay the building, if there is one.
            let Some(pledge) = job.then else {
                return false;
            };
            // A footprint with tiles given up on can't be built on.
            if job.skip[..usize::from(job.skipped)]
                .iter()
                .any(|&t| left(t))
            {
                self.drop(r, *job);
                return false;
            }
            return self.leave(regs, unit, r, job, pledge);
        };
        let (x, y) = ((t % side) as i32, (t / side) as i32);
        let way = Approach::of(regs, unit, r, [x, y, 1, 1]);
        if way.gap > way.reach() {
            if regs.target[r].is_none() && job.clock == 0 {
                if way.gap >= job.gap {
                    job.stalls += 1;
                    if job.stalls >= STALLS {
                        // Can't get at it: give it up.
                        if usize::from(job.skipped) == SKIPS {
                            self.drop(r, *job);
                            return false;
                        }
                        job.skip[usize::from(job.skipped)] = t;
                        job.skipped += 1;
                        job.next();
                        return true;
                    }
                } else {
                    job.stalls = 0;
                }
                job.gap = way.gap;
                job.clock = RETRY_TICKS;
                way.walk(regs, r);
            }
            return true;
        }
        way.arrive(regs, r);
        let full =
            crate::conquest::at_tempo(clear_ticks(terrain.growth(x, y)), self.percent) * HAND;
        let k = match self.worked.binary_search_by_key(&t, |w| w.0) {
            Ok(k) => k,
            Err(k) => {
                self.worked.insert(k, (t, full));
                k
            }
        };
        let work = HAND * u32::from(regs.steps(r));
        self.worked[k].1 = self.worked[k].1.saturating_sub(work);
        if self.worked[k].1 == 0 {
            self.worked.remove(k);
            self.done.push(Done::Tile {
                x,
                y,
                owner: regs.owner[r],
            });
            job.next();
        }
        true
    }

    /// The overgrown tile of `job`'s area nearest `here` that `left` says
    /// is still to clear, and that the worker hasn't given up on.
    fn nearest(
        &self,
        here: FxVec2,
        terrain: &Terrain,
        job: &Job,
        left: &impl Fn(u32) -> bool,
    ) -> Option<u32> {
        let side = terrain.growth_side();
        let [ax, ay, w, d] = job.area;
        let skip = &job.skip[..usize::from(job.skipped)];
        (ay..ay + d)
            .flat_map(|y| (ax..ax + w).map(move |x| (x, y)))
            .filter(|&(x, y)| x >= 0 && y >= 0 && x < side as i32 && y < side as i32)
            .map(|(x, y)| (y as u32 * side + x as u32, (x, y)))
            .filter(|&(t, _)| left(t) && !skip.contains(&t))
            .min_by_key(|&(t, (x, y))| {
                let centre = footprint_centre([x, y, 1, 1]);
                ((centre - here).length_sq_raw(), t)
            })
            .map(|(t, _)| t)
    }

    /// The footprint is cleared: worker slot `r` steps off it, then lays
    /// `pledge`. Returns whether the order goes on.
    fn leave(
        &mut self,
        regs: &mut Regiments,
        unit: &UnitStats,
        r: usize,
        job: &mut Job,
        pledge: Pledge,
    ) -> bool {
        let [x, y, w, d] = job.area;
        let (_, depth) = unit.block_size_in(regs.formation[r], regs.files[r], regs.models[r]);
        let half = Fx::from_raw(depth.raw() / 2);
        let here = regs.position[r];
        // Off the footprint by half the block and a tile, on its nearest
        // side.
        let room = half + Fx::ONE;
        let (lo, hi) = (FxVec2::from_ints(x, y), FxVec2::from_ints(x + w, y + d));
        let inside = here.x > lo.x - room
            && here.y > lo.y - room
            && here.x < hi.x + room
            && here.y < hi.y + room;
        if !inside {
            regs.target[r] = None;
            self.job[r] = None;
            self.done.push(Done::Lay { r, pledge });
            return false;
        }
        if regs.target[r].is_some() || job.clock > 0 {
            return true;
        }
        let out = room + Fx::ONE / 4;
        let (_, out) = [
            (here.x - lo.x, FxVec2::new(lo.x - out, here.y)),
            (hi.x - here.x, FxVec2::new(hi.x + out, here.y)),
            (here.y - lo.y, FxVec2::new(here.x, lo.y - out)),
            (hi.y - here.y, FxVec2::new(here.x, hi.y + out)),
        ]
        .into_iter()
        .min_by_key(|&(gap, _)| gap)
        .expect("four sides");
        job.stalls += 1;
        if job.stalls > STALLS {
            self.drop(r, *job);
            return false;
        }
        job.clock = RETRY_TICKS;
        let h = regs.handle(r);
        regs.order(
            h,
            Order {
                target: out,
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            },
        );
        true
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        for r in 0..self.job.len() {
            let Some(job) = self.live(regs, r) else {
                continue;
            };
            h.u32(r as u32);
            for v in job.area {
                h.i32(v);
            }
            h.u32(job.at.unwrap_or(u32::MAX));
            h.u16(job.clock);
            h.i32(job.gap.raw());
            h.u8(job.stalls);
            h.u8(job.skipped);
            for &t in &job.skip[..usize::from(job.skipped)] {
                h.u32(t);
            }
            if let Some(p) = job.then {
                h.u8(p.owner);
                h.u16(p.building);
                h.i32(p.tile.0);
                h.i32(p.tile.1);
                h.u8(p.turn);
            }
        }
        h.u32(self.worked.len() as u32);
        for &(t, left) in &self.worked {
            h.u32(t);
            h.u32(left);
        }
    }
}
