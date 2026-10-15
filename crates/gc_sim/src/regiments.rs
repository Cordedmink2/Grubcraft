use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{Combat, PATIENCE};
use crate::formation::{self, MAX_MODELS};
use crate::morale::{self, MoraleState};
use crate::nav::Nav;
use crate::terrain::Terrain;
use crate::{Formation, PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitId, UnitStats};

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

/// Where a regiment is in its rhythm of melee blows (see `Combat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Strike {
    /// Strikes on its turn, once a second.
    #[default]
    Turn,
    /// Spears or pikes that have just met their foe: strike this tick.
    Early,
    /// Struck early, so it sits out its next turn: long weapons strike
    /// sooner, never more often.
    Rest,
}

/// When the travel direction is further than this from the facing, the
/// regiment turns on the spot before marching (45 degrees).
const TURN_BEFORE_MARCH: i32 = 0x2000;
/// An order with an explicit facing that ends within this distance is done as
/// a shuffle: the block turns toward its final facing and steps there at half
/// pace instead of wheeling round to face the way it walks.
const SHUFFLE_DISTANCE: Fx = Fx::from_int(4);

/// With nothing blocked within this many tiles there is always room to
/// pass beside a friend, so the queuing check is skipped.
const OPEN_GROUND: i32 = 10;

/// Ticks between looks at whether the way ahead has cleared, while queuing.
const RECHECK: u16 = 4;
/// The most ticks a regiment marches without looking for friends in its
/// way (see `clear`).
const CLEAR_TICKS: u8 = 8;

/// Structure-of-arrays pool of regiments with a fixed capacity.
///
/// Every array is allocated to full capacity up front; spawning and freeing
/// only flip flags and push/pop a free list that never grows past capacity.
pub struct Regiments {
    pub(crate) alive: Vec<bool>,
    pub(crate) generation: Vec<u16>,
    pub(crate) owner: Vec<PlayerId>,
    pub(crate) unit: Vec<UnitId>,
    /// Centre of the block.
    pub(crate) position: Vec<FxVec2>,
    pub(crate) facing: Vec<Angle>,
    /// Soldiers per rank.
    pub(crate) files: Vec<u16>,
    /// How closely the soldiers stand.
    pub(crate) formation: Vec<Formation>,
    /// Soldiers still standing.
    pub(crate) models: Vec<u16>,
    /// Hit points per soldier, `MAX_MODELS` per regiment, by formation slot
    /// (slot `k` is rank `k / files`, file `k % files`; rank 0 is the front).
    pub(crate) hp: Vec<u16>,
    /// Full marching speed, tiles per tick.
    pub(crate) speed: Vec<Fx>,
    /// Turn rate, angle steps per tick.
    pub(crate) turn: Vec<u16>,
    pub(crate) target: Vec<Option<FxVec2>>,
    pub(crate) target_facing: Vec<Option<Angle>>,
    /// Speed for the current order, tiles per tick.
    pub(crate) pace: Vec<Fx>,
    /// Enemy regiment this one was ordered to attack.
    pub(crate) attack_target: Vec<Option<RegimentHandle>>,
    /// The enemy it is fighting in melee this tick (the nearest one in contact).
    pub(crate) engaged: Vec<Option<RegimentHandle>>,
    /// Ticks of charge bonus left.
    pub(crate) charge: Vec<u16>,
    pub(crate) strike: Vec<Strike>,
    /// Ticks until a ranged regiment can loose its next volley.
    pub(crate) reload: Vec<u16>,
    /// Damage taken but not yet a whole hit point, in 1/256ths.
    pub(crate) damage_carry: Vec<u16>,
    /// Soldiers at full strength (the unit's), for casualty fractions.
    pub(crate) full_models: Vec<u16>,
    /// Morale in tenths of a point (see [`crate::morale`]), and what it means.
    pub(crate) morale: Vec<i16>,
    pub(crate) morale_state: Vec<MoraleState>,
    /// Soldiers lost, and flank (bit 0) or rear (bit 1) hits taken, since the
    /// regiment's last morale check.
    pub(crate) losses: Vec<u16>,
    pub(crate) shock: Vec<u8>,
    /// While routing: ticks spent with no enemy near.
    pub(crate) rout_clock: Vec<u16>,
    /// Tick the regiment last broke and ran, plus one (0: never).
    pub(crate) last_rout: Vec<u32>,
    /// Ticks spent waiting behind a friendly block in its way (see
    /// [`crate::combat::Combat::friend_in_the_way`]); 0 while marching freely.
    pub(crate) held: Vec<u16>,
    /// Ticks it can march before looking for friends in its way again:
    /// none holding their ground were near enough to reach sooner.
    pub(crate) clear: Vec<u8>,
    /// How deep the block overlaps its friends, in tiles (see
    /// `Combat::crowding`).
    pub(crate) crowd: Vec<Fx>,
    /// Ticks left until it is back in order (0: in order).
    pub(crate) disorder: Vec<u8>,
    /// Free slot indices, popped from the back. Initialised so slot 0 is used first.
    pub(crate) free: Vec<u32>,
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
            formation: vec![Formation::Line; n],
            models: vec![0; n],
            hp: vec![0; n * MAX_MODELS as usize],
            speed: vec![Fx::ZERO; n],
            turn: vec![0; n],
            target: vec![None; n],
            target_facing: vec![None; n],
            pace: vec![Fx::ZERO; n],
            attack_target: vec![None; n],
            engaged: vec![None; n],
            charge: vec![0; n],
            strike: vec![Strike::Turn; n],
            reload: vec![0; n],
            damage_carry: vec![0; n],
            full_models: vec![0; n],
            morale: vec![0; n],
            morale_state: vec![MoraleState::Steady; n],
            losses: vec![0; n],
            shock: vec![0; n],
            rout_clock: vec![0; n],
            last_rout: vec![0; n],
            held: vec![0; n],
            clear: vec![0; n],
            crowd: vec![Fx::ZERO; n],
            disorder: vec![0; n],
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
        self.formation[i] = Formation::Line;
        self.models[i] = stats.models;
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        hp.fill(0);
        hp[..stats.models as usize].fill(stats.hp_per_model);
        self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
        self.turn[i] = (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u32::from(u16::MAX)) as u16;
        self.target[i] = None;
        self.target_facing[i] = None;
        self.pace[i] = self.speed[i];
        self.attack_target[i] = None;
        self.engaged[i] = None;
        self.charge[i] = 0;
        self.strike[i] = Strike::Turn;
        self.reload[i] = 0;
        self.damage_carry[i] = 0;
        self.full_models[i] = stats.models;
        self.morale[i] = morale::fresh(stats);
        self.morale_state[i] = MoraleState::Steady;
        self.losses[i] = 0;
        self.shock[i] = 0;
        self.rout_clock[i] = 0;
        self.last_rout[i] = 0;
        self.held[i] = 0;
        self.clear[i] = 0;
        self.crowd[i] = Fx::ZERO;
        self.disorder[i] = 0;
        Some(RegimentHandle {
            index,
            generation: self.generation[i],
        })
    }

    /// Give every living regiment of type `unit` its new speed and turn rate
    /// (the rest of its stats are read from the unit table as needed).
    pub(crate) fn retune(&mut self, unit: UnitId, stats: &UnitStats) {
        for i in 0..self.alive.len() {
            if !self.alive[i] || self.unit[i] != unit {
                continue;
            }
            let old_speed = self.speed[i];
            self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
            self.turn[i] =
                (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u32::from(u16::MAX)) as u16;
            // A regiment marching at full speed keeps doing so; a slower
            // ordered pace is kept, but never above the new top speed.
            self.pace[i] = if self.pace[i] == old_speed {
                self.speed[i]
            } else {
                self.pace[i].min(self.speed[i])
            };
            // Shields taken away: the wall comes apart.
            if !stats.can_form(self.formation[i]) {
                self.formation[i] = Formation::Line;
            }
        }
    }

    /// Change regiment `i`'s formation. Routing regiments don't listen, and
    /// only shielded ones can form a shield wall.
    pub(crate) fn set_formation(
        &mut self,
        h: RegimentHandle,
        formation: Formation,
        units: &[UnitStats],
    ) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        // Ranks re-form out of the fight: locked in melee, or running, a
        // regiment can't change them.
        if self.morale_state[i].broken()
            || self.engaged[i].is_some()
            || !units[self.unit[i] as usize].can_form(formation)
        {
            return false;
        }
        self.formation[i] = formation;
        true
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

    pub(crate) fn resolve(&self, h: RegimentHandle) -> Option<usize> {
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

    /// Throw regiment `i` into full disorder, as pushing through friends
    /// does (other combat rules may too); it re-forms over
    /// [`DISORDER_TICKS`](crate::combat::DISORDER_TICKS) once nothing keeps
    /// it there.
    pub(crate) fn disorder(&mut self, i: usize) {
        self.disorder[i] = crate::combat::DISORDER_TICKS;
    }

    /// Give a move order. Routing regiments don't listen.
    pub fn order(&mut self, h: RegimentHandle, order: Order) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.morale_state[i].broken() {
            return false;
        }
        self.target[i] = Some(order.target);
        self.target_facing[i] = order.facing;
        self.attack_target[i] = None;
        self.held[i] = 0;
        self.clear[i] = 0;
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

    /// Order a regiment to attack an enemy regiment: melee regiments close
    /// in and fight, ranged ones move into range and shoot.
    pub fn attack(&mut self, h: RegimentHandle, target: RegimentHandle) -> bool {
        let (Some(i), Some(t)) = (self.resolve(h), self.resolve(target)) else {
            return false;
        };
        if self.owner[i] == self.owner[t] || self.morale_state[i].broken() {
            return false;
        }
        self.attack_target[i] = Some(target);
        self.target_facing[i] = None;
        self.held[i] = 0;
        self.clear[i] = 0;
        self.pace[i] = self.speed[i];
        true
    }

    /// Take `amount` hit points off the regiment, from the back ranks forward:
    /// the last slot loses hit points first, and each soldier who falls takes
    /// the next one's place in the damage. Returns soldiers lost.
    pub(crate) fn take_damage(&mut self, i: usize, mut amount: u32) -> u16 {
        let before = self.models[i];
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        while amount > 0 && self.models[i] > 0 {
            let k = self.models[i] as usize - 1;
            if u32::from(hp[k]) > amount {
                hp[k] -= amount as u16;
                amount = 0;
            } else {
                amount -= u32::from(hp[k]);
                hp[k] = 0;
                self.models[i] -= 1;
            }
        }
        self.files[i] = self.files[i].min(self.models[i]).max(1);
        let lost = before - self.models[i];
        self.losses[i] = self.losses[i].saturating_add(lost);
        lost
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
            formation: self.formation[i],
            models: self.models[i],
            moving: self.target[i].is_some() || self.target_facing[i].is_some(),
            engaged: self.engaged[i].is_some(),
            charge: self.charge[i],
            morale: self.morale_state[i],
            morale_points: (self.morale[i].max(0) / morale::SCALE) as u8,
            disorder: self.disorder[i],
        }
    }

    /// Live regiments in index order, for presentation and tests.
    pub fn iter(&self) -> impl Iterator<Item = RegimentView> + '_ {
        (0..self.alive.len())
            .filter(|&i| self.alive[i])
            .map(|i| self.view(i))
    }

    pub(crate) fn step_movement(
        &mut self,
        nav: &mut Nav,
        terrain: &Terrain,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
    ) {
        for i in 0..self.alive.len() {
            if !self.alive[i] {
                continue;
            }
            if self.target[i].is_none() {
                self.held[i] = 0;
                self.clear[i] = 0;
            }
            let turn = self.turn[i];
            if self.engaged[i].is_some() {
                // Locked in melee: only turn toward the enemy (set by combat).
                if let Some(want) = self.target_facing[i] {
                    self.facing[i] = self.facing[i].turn_toward(want, turn / 2);
                }
                continue;
            }
            if let Some(target) = self.target[i] {
                // The next waypoint on the way to the target; none while the
                // route is being searched for.
                let Some((point, last)) = nav.steer(i, target) else {
                    continue;
                };
                let delta = point - self.position[i];
                let dist = delta.length();
                // A shield wall shuffles along at half its speed.
                let pace = match self.formation[i].pace_percent() {
                    100 => self.pace[i],
                    p => self.pace[i].min(Fx::from_raw((self.speed[i].raw() * p / 100).max(1))),
                };
                let shuffle = last && dist <= SHUFFLE_DISTANCE && self.target_facing[i].is_some();
                let step = if shuffle {
                    self.facing[i] =
                        self.facing[i].turn_toward(self.target_facing[i].unwrap(), turn);
                    Fx::from_raw(pace.raw() / 2).max(Fx::from_raw(1))
                } else {
                    let travel = Angle::of_vec(delta);
                    let off = i32::from(self.facing[i].delta_to(travel));
                    self.facing[i] = self.facing[i].turn_toward(travel, turn);
                    if off.abs() > TURN_BEFORE_MARCH {
                        continue;
                    }
                    pace
                };
                let step = terrain.wading(step, self.position[i]);
                let step = crate::combat::pushing(step, self.crowd[i]);
                debug_assert!(step <= combat.max_step(), "a step outruns the crowd check");
                let here = self.position[i];
                let arriving = dist <= step;
                let dir = if dist.raw() > 0 {
                    FxVec2::new(delta.x / dist, delta.y / dist)
                } else {
                    FxVec2::ZERO
                };
                let next = if arriving { point } else { here + dir * step };
                // A friendly block holding its ground in the way, where the
                // ground leaves no room beside it (a ford, a pass): wait
                // behind it. In the open, blocks still pass through.
                // A regiment already waiting looks again only every few
                // ticks.
                if (1..PATIENCE).contains(&self.held[i]) && !self.held[i].is_multiple_of(RECHECK) {
                    self.held[i] += 1;
                    continue;
                }
                if self.clear[i] > 0 {
                    self.clear[i] -= 1;
                } else if !nav.open_around(here, OPEN_GROUND) {
                    match combat.friend_in_the_way(self, units, stances, i, next) {
                        Ok(j) if !combat.room_beside(self, units, nav, i, j, dir) => {
                            self.held[i] = self.held[i].saturating_add(1);
                            continue;
                        }
                        Ok(_) => {}
                        Err(gap) => {
                            let ticks = gap.raw() / step.raw().max(1);
                            self.clear[i] = ticks.clamp(0, i32::from(CLEAR_TICKS)) as u8;
                        }
                    }
                }
                // Marching freely again. Once its patience ran out it stays
                // out until this order is done, so it can finish slipping
                // past whoever was in the way.
                if self.held[i] < PATIENCE {
                    self.held[i] = 0;
                }
                if arriving {
                    self.position[i] = point;
                    if last {
                        self.target[i] = None;
                    } else {
                        nav.reached(i);
                    }
                } else if nav.blocks(here, next) {
                    nav.lost(i);
                } else {
                    self.position[i] = next;
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
            h.u8(u8::from(self.alive[i]));
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
            // Only when not the plain line, so worlds where nobody changes
            // formation hash as they did before formations existed.
            if self.formation[i] != Formation::Line {
                h.u8(self.formation[i] as u8);
            }
            h.u16(self.models[i]);
            for &hp in &self.hp[i * MAX_MODELS as usize..][..self.models[i] as usize] {
                h.u16(hp);
            }
            h.i32(self.speed[i].raw());
            h.u16(self.turn[i]);
            h.i32(self.pace[i].raw());
            hash_handle(h, self.attack_target[i]);
            hash_handle(h, self.engaged[i]);
            h.u16(self.charge[i]);
            // Only when set, as for the formation above.
            if self.strike[i] == Strike::Rest {
                h.u8(1);
            }
            h.u16(self.reload[i]);
            h.u16(self.damage_carry[i]);
            h.u16(self.full_models[i]);
            h.u16(self.morale[i] as u16);
            h.u8(self.morale_state[i] as u8);
            h.u16(self.losses[i]);
            h.u8(self.shock[i]);
            h.u16(self.rout_clock[i]);
            h.u32(self.last_rout[i]);
            // Only while overlapping or re-forming, so worlds where no
            // friends ever overlap hash as they did before.
            if self.crowd[i] != Fx::ZERO || self.disorder[i] != 0 {
                h.i32(self.crowd[i].raw());
                h.u8(self.disorder[i]);
            }
            // Only while waiting, so worlds where nobody ever queues hash
            // as they did before queuing existed.
            if self.held[i] != 0 || self.clear[i] != 0 {
                h.u16(self.held[i]);
                h.u8(self.clear[i]);
            }
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

pub(crate) fn hash_handle(h: &mut StateHasher, handle: Option<RegimentHandle>) {
    match handle {
        Some(r) => {
            h.u8(1);
            h.u32(r.index);
            h.u16(r.generation);
        }
        None => h.u8(0),
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
    /// How closely the soldiers stand.
    pub formation: Formation,
    /// Soldiers still standing.
    pub models: u16,
    /// Walking or turning to an ordered facing.
    pub moving: bool,
    /// Fighting in melee.
    pub engaged: bool,
    /// Ticks of charge bonus left: it hit home at speed within the last
    /// few seconds.
    pub charge: u16,
    pub morale: MoraleState,
    /// Morale in points (0 to 100); meaningless for regiments that never rout.
    pub morale_points: u8,
    /// Out of order from pushing through friends: ticks left until it has
    /// re-formed, from [`DISORDER_TICKS`](crate::combat::DISORDER_TICKS)
    /// (full disorder) down to 0 (in order).
    pub disorder: u8,
}

impl RegimentView {
    pub fn ranks(&self) -> u16 {
        formation::ranks(self.files, self.models)
    }
}
