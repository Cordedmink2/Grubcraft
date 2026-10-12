use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::{PlayerId, StateHasher, UnitId};

/// Generational handle to a regiment. Stale handles (to a slot that has since
/// been freed and reused) are detected and rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegimentHandle {
    pub index: u32,
    pub generation: u16,
}

/// Structure-of-arrays pool of regiments with a fixed capacity.
///
/// Every array is allocated to full capacity up front; spawning and freeing
/// only flip flags and push/pop a free list that never grows past capacity.
pub struct Regiments {
    alive: Vec<bool>,
    generation: Vec<u16>,
    owner: Vec<PlayerId>,
    unit: Vec<UnitId>,
    position: Vec<FxVec2>,
    /// Unit vector the regiment is facing (new regiments face north, -y).
    facing: Vec<FxVec2>,
    /// Tiles per tick.
    speed: Vec<Fx>,
    target: Vec<Option<FxVec2>>,
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
            facing: vec![FxVec2::new(Fx::ZERO, -Fx::ONE); n],
            speed: vec![Fx::ZERO; n],
            target: vec![None; n],
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

    pub fn spawn(
        &mut self,
        owner: PlayerId,
        unit: UnitId,
        position: FxVec2,
        speed: Fx,
    ) -> Option<RegimentHandle> {
        let index = self.free.pop()?;
        let i = index as usize;
        self.alive[i] = true;
        self.owner[i] = owner;
        self.unit[i] = unit;
        self.position[i] = position;
        self.facing[i] = FxVec2::new(Fx::ZERO, -Fx::ONE);
        self.speed[i] = speed;
        self.target[i] = None;
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

    pub fn set_target(&mut self, h: RegimentHandle, target: FxVec2) -> bool {
        match self.resolve(h) {
            Some(i) => {
                self.target[i] = Some(target);
                true
            }
            None => false,
        }
    }

    /// Live regiments in index order, for presentation and tests.
    pub fn iter(&self) -> impl Iterator<Item = RegimentView> + '_ {
        (0..self.alive.len())
            .filter(|&i| self.alive[i])
            .map(|i| RegimentView {
                handle: RegimentHandle {
                    index: i as u32,
                    generation: self.generation[i],
                },
                owner: self.owner[i],
                unit: self.unit[i],
                position: self.position[i],
                facing: self.facing[i],
            })
    }

    pub(crate) fn step_movement(&mut self) {
        for i in 0..self.alive.len() {
            if !self.alive[i] {
                continue;
            }
            let Some(target) = self.target[i] else {
                continue;
            };
            let delta = target - self.position[i];
            let dist = delta.length();
            let speed = self.speed[i];
            if dist <= speed {
                self.position[i] = target;
                self.target[i] = None;
            } else {
                let dir = FxVec2::new(delta.x / dist, delta.y / dist);
                self.position[i] += dir * speed;
                self.facing[i] = dir;
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
            h.i32(self.facing[i].x.raw());
            h.i32(self.facing[i].y.raw());
            h.i32(self.speed[i].raw());
            match self.target[i] {
                Some(t) => {
                    h.u8(1);
                    h.i32(t.x.raw());
                    h.i32(t.y.raw());
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
    pub position: FxVec2,
    pub facing: FxVec2,
}
