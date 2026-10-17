//! Conquest training (docs/21-buildings.md): each building's queue of
//! regiments to raise, paid for when queued, and where they go once out.
//!
//! - A building trains the unit types whose `ConquestUnit::trained_at` is
//!   it or a building it is a tier upgrade of, and only those its owner's
//!   tier allows (see [`crate::structures::Structures::tier_of`]).
//! - A queue holds [`QUEUE`] regiments. The one at the front starts once
//!   its owner has the supply for it, counting those already in training,
//!   and takes its `train_ticks` at the match's tempo.
//! - Units that hatch from larvae (the Brood's, see [`crate::larvae`])
//!   hatch all at once: every one in the queue starts as soon as there is
//!   supply for it, not only the front one. They start in queue order: one
//!   waiting for supply holds up those behind it.
//! - A finished regiment comes out at the building's south side (or another
//!   where that is blocked) and marches to its rally point, if it has one.
//! - Calling off the last in a queue gives back all it cost. A building
//!   that falls takes its queue with it, and gives back nothing.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::conquest::ConquestUnit;
use crate::nav::Nav;
use crate::structures::{StructureHandle, Structures};
use crate::{MAX_PARTIES, PlayerId, StateHasher, UnitId};

/// Regiments a building can have queued.
pub const QUEUE: usize = 5;

/// A regiment that finished training, for the world to put on the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Trained {
    pub(crate) owner: PlayerId,
    pub(crate) unit: UnitId,
    pub(crate) at: FxVec2,
    pub(crate) rally: Option<FxVec2>,
}

/// Whether a regiment of type `unit` hatches from a larva.
fn hatches(units: &[ConquestUnit], unit: UnitId) -> bool {
    units.get(usize::from(unit)).is_some_and(|u| u.hatches)
}

/// What a regiment of type `unit` takes of its owner's supply.
fn supply(units: &[ConquestUnit], unit: UnitId) -> u32 {
    units
        .get(usize::from(unit))
        .map_or(0, |u| u32::from(u.supply))
}

/// Every building's training queue and rally point, by building slot.
pub struct Production {
    build_percent: u32,
    /// The generation of the building each slot's queue belongs to: a slot
    /// reused by a new building starts empty.
    queue_gen: Vec<u16>,
    queue: Vec<[UnitId; QUEUE]>,
    queued: Vec<u8>,
    /// Ticks left for each regiment in the queue; 0 until it starts. Only
    /// the front one trains, unless they hatch.
    clock: Vec<[u32; QUEUE]>,
    rally: Vec<Option<FxVec2>>,
    /// Regiments that finished training in the last tick.
    pub(crate) trained: Vec<Trained>,
}

impl Production {
    pub(crate) fn new(structures: usize, build_percent: u32) -> Production {
        Production {
            build_percent: build_percent.clamp(10, 1000),
            queue_gen: vec![0; structures],
            queue: vec![[0; QUEUE]; structures],
            queued: vec![0; structures],
            clock: vec![[0; QUEUE]; structures],
            rally: vec![None; structures],
            trained: Vec::with_capacity(structures),
        }
    }

    /// Building `h`'s record, emptied first if its slot has a new building.
    fn slot(&mut self, h: StructureHandle) -> usize {
        let i = h.index as usize;
        if self.queue_gen[i] != h.generation {
            self.queue_gen[i] = h.generation;
            self.queued[i] = 0;
            self.clock[i] = [0; QUEUE];
            self.rally[i] = None;
        }
        i
    }

    /// Building `h`'s slot, if its record is for that building.
    fn live(&self, h: StructureHandle) -> Option<usize> {
        let i = h.index as usize;
        (self.queue_gen.get(i) == Some(&h.generation)).then_some(i)
    }

    /// What building `h` has queued, front first.
    pub fn queue_of(&self, h: StructureHandle) -> &[UnitId] {
        self.live(h)
            .map_or(&[], |i| &self.queue[i][..usize::from(self.queued[i])])
    }

    /// Ticks left for the regiment at the front of building `h`'s queue,
    /// once it has started.
    pub fn front_ticks_left(&self, h: StructureHandle) -> Option<u32> {
        self.ticks_left(h, 0)
    }

    /// Ticks left for regiment `k` of building `h`'s queue, once it has
    /// started.
    pub fn ticks_left(&self, h: StructureHandle, k: usize) -> Option<u32> {
        let i = self.live(h)?;
        (k < usize::from(self.queued[i]))
            .then(|| self.clock[i][k])
            .filter(|&t| t > 0)
    }

    /// How far regiment `k` of building `h`'s queue has trained, in
    /// percent: 0 until it starts.
    pub fn percent(&self, h: StructureHandle, k: usize, units: &[ConquestUnit]) -> Option<u8> {
        let &unit = self.queue_of(h).get(k)?;
        let full = self.train_ticks(units, unit);
        let left = self.ticks_left(h, k).unwrap_or(full).min(full);
        Some((100 - left * 100 / full) as u8)
    }

    /// Ticks a regiment of type `unit` takes to train at this tempo.
    fn train_ticks(&self, units: &[ConquestUnit], unit: UnitId) -> u32 {
        let ticks = units
            .get(usize::from(unit))
            .map_or(1, |u| u32::from(u.train_ticks));
        crate::conquest::at_tempo(ticks, self.build_percent)
    }

    /// Building `h`'s rally point.
    pub fn rally_of(&self, h: StructureHandle) -> Option<FxVec2> {
        self.rally[self.live(h)?]
    }

    /// Whether building `h` has room in its queue.
    pub(crate) fn has_room(&self, h: StructureHandle) -> bool {
        self.queue_of(h).len() < QUEUE
    }

    /// Add `unit` to building `h`'s queue; the caller has checked it may
    /// and taken the price.
    pub(crate) fn enqueue(&mut self, h: StructureHandle, unit: UnitId) {
        let i = self.slot(h);
        let n = usize::from(self.queued[i]);
        self.queue[i][n] = unit;
        self.queued[i] += 1;
    }

    /// Take the last regiment off building `h`'s queue, for its price back.
    pub(crate) fn dequeue(&mut self, h: StructureHandle) -> Option<UnitId> {
        let i = self.slot(h);
        let n = usize::from(self.queued[i]);
        if n == 0 {
            return None;
        }
        self.queued[i] -= 1;
        self.clock[i][n - 1] = 0;
        Some(self.queue[i][n - 1])
    }

    pub(crate) fn set_rally(&mut self, h: StructureHandle, at: FxVec2) {
        let i = self.slot(h);
        self.rally[i] = Some(at);
    }

    /// Add the supply regiments in training take to `used`, by player.
    pub(crate) fn in_training(
        &self,
        structures: &Structures,
        units: &[ConquestUnit],
        used: &mut [u32; MAX_PARTIES],
    ) {
        for i in 0..self.queued.len() {
            if self.queued[i] > 0
                && let Some(owner) = structures.live_slot(i, self.queue_gen[i])
                && let Some(u) = used.get_mut(usize::from(owner))
            {
                for k in 0..usize::from(self.queued[i]) {
                    if self.clock[i][k] > 0 {
                        *u += supply(units, self.queue[i][k]);
                    }
                }
            }
        }
    }

    /// One tick of training. `used` gives each player's supply in use by
    /// their regiments, worked out only if a queue is waiting to start.
    pub(crate) fn update(
        &mut self,
        structures: &Structures,
        nav: &Nav,
        units: &[ConquestUnit],
        used: impl FnOnce() -> [u32; MAX_PARTIES],
    ) {
        self.trained.clear();
        let mut used = Some(used);
        let mut room: Option<[u32; MAX_PARTIES]> = None;
        for i in 0..self.queued.len() {
            if self.queued[i] == 0 {
                continue;
            }
            let Some(owner) = structures.live_slot(i, self.queue_gen[i]) else {
                // It fell: the queue goes with it.
                self.queued[i] = 0;
                self.clock[i] = [0; QUEUE];
                continue;
            };
            let mut k = 0;
            while k < usize::from(self.queued[i]) {
                let unit = self.queue[i][k];
                // Behind the front, only what hatches is under way.
                if k > 0 && !hatches(units, unit) {
                    break;
                }
                if self.clock[i][k] == 0 {
                    // Supply left under each player's cap, counting those
                    // in training.
                    let room = room.get_or_insert_with(|| {
                        let mut taken = used.take().map_or([0; MAX_PARTIES], |f| f());
                        self.in_training(structures, units, &mut taken);
                        // Out this tick, but not on the field until it ends.
                        for t in &self.trained {
                            if let Some(n) = taken.get_mut(usize::from(t.owner)) {
                                *n += supply(units, t.unit);
                            }
                        }
                        core::array::from_fn(|p| {
                            structures
                                .supply_cap(p as PlayerId)
                                .saturating_sub(taken[p])
                        })
                    });
                    let need = supply(units, unit);
                    let Some(left) = room.get_mut(usize::from(owner)).filter(|l| **l >= need)
                    else {
                        // Waits for room, and those behind it wait too.
                        break;
                    };
                    *left -= need;
                    self.clock[i][k] = self.train_ticks(units, unit);
                }
                self.clock[i][k] -= 1;
                if self.clock[i][k] > 0 {
                    k += 1;
                    continue;
                }
                self.trained.push(Trained {
                    owner,
                    unit,
                    at: structures.door(i, nav),
                    rally: self.rally[i],
                });
                self.queue[i].copy_within(k + 1.., k);
                self.clock[i].copy_within(k + 1.., k);
                self.clock[i][QUEUE - 1] = 0;
                self.queued[i] -= 1;
                if !hatches(units, unit) {
                    // The next starts next tick.
                    break;
                }
            }
        }
    }

    pub(crate) fn hash_into(&self, structures: &Structures, h: &mut StateHasher) {
        h.u32(self.build_percent);
        for i in 0..self.queued.len() {
            let live = structures.live_slot(i, self.queue_gen[i]).is_some();
            if !live || (self.queued[i] == 0 && self.rally[i].is_none()) {
                h.u8(0);
                continue;
            }
            h.u8(self.queued[i] + 1);
            for k in 0..usize::from(self.queued[i]) {
                h.u16(self.queue[i][k]);
                h.u32(self.clock[i][k]);
            }
            match self.rally[i] {
                Some(at) => {
                    h.u8(1);
                    h.i32(at.x.raw());
                    h.i32(at.y.raw());
                }
                None => h.u8(0),
            }
        }
    }
}
