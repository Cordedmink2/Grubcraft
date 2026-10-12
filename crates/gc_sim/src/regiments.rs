use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::formation::{self, MAX_MODELS};
use crate::{PlayerId, StateHasher, TICKS_PER_SECOND, UnitId, UnitStats};

/// Generational handle to a regiment. Stale handles (to a slot that has since
/// been freed and reused) are detected and rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegimentHandle {
    pub index: u32,
    pub generation: u16,
}

/// A move order for one regiment, already validated by [`crate::World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    pub target: FxVec2,
    /// Facing on arrival; `None` keeps the direction of travel.
    pub facing: Option<Angle>,
    /// New number of files (frontage in soldiers); 0 keeps the current one.
    pub files: u16,
    /// Tiles per second; zero or negative means the regiment's own speed.
    pub pace: Fx,
}

/// When the travel direction is further than this from the facing, the
/// regiment turns on the spot before marching (45 degrees).
const TURN_BEFORE_MARCH: i32 = 0x2000;
/// An order with an explicit facing that ends within this distance is done as
/// a shuffle: the block turns toward its final facing and steps there at half
/// pace instead of wheeling round to face the way it walks.
const SHUFFLE_DISTANCE: Fx = Fx::from_int(4);

/// Structure-of-arrays pool of regiments with a fixed capacity.
///
/// Every array is allocated to full capacity up front; spawning and freeing
/// only flip flags and push/pop a free list that never grows past capacity.
pub struct Regiments {
    alive: Vec<bool>,
    generation: Vec<u16>,
    owner: Vec<PlayerId>,
    unit: Vec<UnitId>,
    /// Centre of the block.
    position: Vec<FxVec2>,
    facing: Vec<Angle>,
    /// Soldiers per rank.
    files: Vec<u16>,
    /// Soldiers still standing.
    models: Vec<u16>,
    /// Hit points per soldier, `MAX_MODELS` per regiment, by formation slot
    /// (slot `k` is rank `k / files`, file `k % files`; rank 0 is the front).
    hp: Vec<u16>,
    /// Full marching speed, tiles per tick.
    speed: Vec<Fx>,
    /// Turn rate, angle steps per tick.
    turn: Vec<u16>,
    target: Vec<Option<FxVec2>>,
    target_facing: Vec<Option<Angle>>,
    /// Speed for the current order, tiles per tick.
    pace: Vec<Fx>,
    /// Free slot indices, popped from the back. Initialised so slot 0 is used first.
    free: Vec<u32>,
}

impl Regiments {
    pub fn with_capacity(capacity: u32) -> Regiments {
        let n = capacity as usize;
        Regiments {
            alive: vec![false; n],
            generation: vec![0; n],
            owner: vec![0; n],
            unit: vec![0; n],
            position: vec![FxVec2::ZERO; n],
            facing: vec![Angle::ZERO; n],
            files: vec![0; n],
            models: vec![0; n],
            hp: vec![0; n * MAX_MODELS as usize],
            speed: vec![Fx::ZERO; n],
            turn: vec![0; n],
            target: vec![None; n],
            target_facing: vec![None; n],
            pace: vec![Fx::ZERO; n],
            free: (0..capacity).rev().collect(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.alive.len()
    }

    pub fn len(&self) -> usize {
        self.capacity() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Create a full-strength regiment. `stats` must already be sanitised
    /// (see [`UnitStats::sanitized`]).
    pub fn spawn(
        &mut self,
        owner: PlayerId,
        unit: UnitId,
        position: FxVec2,
        facing: Angle,
        stats: &UnitStats,
    ) -> Option<RegimentHandle> {
        let index = self.free.pop()?;
        let i = index as usize;
        self.alive[i] = true;
        self.owner[i] = owner;
        self.unit[i] = unit;
        self.position[i] = position;
        self.facing[i] = facing;
        self.files[i] = stats.files;
        self.models[i] = stats.models;
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        hp.fill(0);
        hp[..stats.models as usize].fill(stats.hp_per_model);
        self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
        self.turn[i] = (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u16::MAX as u32) as u16;
        self.target[i] = None;
        self.target_facing[i] = None;
        self.pace[i] = self.speed[i];
        Some(RegimentHandle {
            index,
            generation: self.generation[i],
        })
    }

    pub fn despawn(&mut self, h: RegimentHandle) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.alive[i] = false;
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.free.push(h.index);
        true
    }

    fn resolve(&self, h: RegimentHandle) -> Option<usize> {
        let i = h.index as usize;
        (i < self.alive.len() && self.alive[i] && self.generation[i] == h.generation).then_some(i)
    }

    pub fn owner(&self, h: RegimentHandle) -> Option<PlayerId> {
        self.resolve(h).map(|i| self.owner[i])
    }

    pub fn position(&self, h: RegimentHandle) -> Option<FxVec2> {
        self.resolve(h).map(|i| self.position[i])
    }

    pub fn get(&self, h: RegimentHandle) -> Option<RegimentView> {
        self.resolve(h).map(|i| self.view(i))
    }

    /// Hit points of each standing soldier, by formation slot.
    pub fn model_hp(&self, h: RegimentHandle) -> Option<&[u16]> {
        self.resolve(h)
            .map(|i| &self.hp[i * MAX_MODELS as usize..][..self.models[i] as usize])
    }

    pub fn order(&mut self, h: RegimentHandle, order: Order) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.target[i] = Some(order.target);
        self.target_facing[i] = order.facing;
        if order.files != 0 {
            self.files[i] = order.files.clamp(1, self.models[i].max(1));
        }
        let pace = Fx::from_raw(order.pace.raw() / TICKS_PER_SECOND as i32);
        self.pace[i] = if pace.raw() <= 0 {
            self.speed[i]
        } else {
            pace.min(self.speed[i])
        };
        true
    }

    fn view(&self, i: usize) -> RegimentView {
        RegimentView {
            handle: RegimentHandle {
                index: i as u32,
                generation: self.generation[i],
            },
            owner: self.owner[i],
            unit: self.unit[i],
            position: self.position[i],
            facing: self.facing[i],
            files: self.files[i],
            models: self.models[i],
            moving: self.target[i].is_some() || self.target_facing[i].is_some(),
        }
    }

    /// Live regiments in index order, for presentation and tests.
    pub fn iter(&self) -> impl Iterator<Item = RegimentView> + '_ {
        (0..self.alive.len())
            .filter(|&i| self.alive[i])
            .map(|i| self.view(i))
    }

    pub(crate) fn step_movement(&mut self) {
        for i in 0..self.alive.len() {
            if !self.alive[i] {
                continue;
            }
            let turn = self.turn[i];
            if let Some(target) = self.target[i] {
                let delta = target - self.position[i];
                let dist = delta.length();
                let pace = self.pace[i];
                let shuffle = dist <= SHUFFLE_DISTANCE && self.target_facing[i].is_some();
                let step = if shuffle {
                    self.facing[i] =
                        self.facing[i].turn_toward(self.target_facing[i].unwrap(), turn);
                    Fx::from_raw(pace.raw() / 2).max(Fx::from_raw(1))
                } else {
                    let travel = Angle::of_vec(delta);
                    let off = self.facing[i].delta_to(travel) as i32;
                    self.facing[i] = self.facing[i].turn_toward(travel, turn);
                    if off.abs() > TURN_BEFORE_MARCH {
                        continue;
                    }
                    pace
                };
                if dist <= step {
                    self.position[i] = target;
                    self.target[i] = None;
                } else {
                    let dir = FxVec2::new(delta.x / dist, delta.y / dist);
                    self.position[i] += dir * step;
                }
            } else if let Some(want) = self.target_facing[i] {
                self.facing[i] = self.facing[i].turn_toward(want, turn);
                if self.facing[i] == want {
                    self.target_facing[i] = None;
                }
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.len() as u32);
        for i in 0..self.alive.len() {
            h.u8(self.alive[i] as u8);
            h.u16(self.generation[i]);
            if !self.alive[i] {
                continue;
            }
            h.u8(self.owner[i]);
            h.u16(self.unit[i]);
            h.i32(self.position[i].x.raw());
            h.i32(self.position[i].y.raw());
            h.u16(self.facing[i].0);
            h.u16(self.files[i]);
            h.u16(self.models[i]);
            for &hp in &self.hp[i * MAX_MODELS as usize..][..self.models[i] as usize] {
                h.u16(hp);
            }
            h.i32(self.speed[i].raw());
            h.u16(self.turn[i]);
            h.i32(self.pace[i].raw());
            match self.target[i] {
                Some(t) => {
                    h.u8(1);
                    h.i32(t.x.raw());
                    h.i32(t.y.raw());
                }
                None => h.u8(0),
            }
            match self.target_facing[i] {
                Some(a) => {
                    h.u8(1);
                    h.u16(a.0);
                }
                None => h.u8(0),
            }
        }
    }
}

/// Read-only snapshot of one regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegimentView {
    pub handle: RegimentHandle,
    pub owner: PlayerId,
    pub unit: UnitId,
    /// Centre of the block.
    pub position: FxVec2,
    pub facing: Angle,
    /// Soldiers per rank.
    pub files: u16,
    /// Soldiers still standing.
    pub models: u16,
    /// Walking or turning to an ordered facing.
    pub moving: bool,
}

impl RegimentView {
    pub fn ranks(&self) -> u16 {
        formation::ranks(self.files, self.models)
    }
}
