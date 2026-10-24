use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{Combat, OPEN_GROUND, PATIENCE};
use crate::formation::{self, MAX_MODELS};
use crate::morale::{self, MoraleState};
use crate::nav::{self, Nav};
use crate::state::State;
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

/// Ticks between looks at whether the way ahead has cleared, while queuing.
const RECHECK: u8 = 4;
/// The most ticks a regiment marches without looking for friends in its
/// way (see `clear`).
const CLEAR_TICKS: u8 = 8;

/// Ticks between steps of narrowing (see [`Regiments::squeeze`]), and
/// between widening by a file once there is room again.
const SQUEEZE_TICKS: u8 = 2;
const WIDEN_TICKS: u8 = 6;
/// Room kept between a squeezed block's outermost soldiers and blocked
/// ground, in tiles.
const SQUEEZE_MARGIN: Fx = Fx::from_ratio(1, 2);
/// Room to spare, beyond that, before a squeezed block widens by a file.
const WIDEN_SPARE: Fx = Fx::from_ratio(1, 2);
/// How far beyond its front rank a marching block looks for narrowing
/// ground, in tiles.
const SQUEEZE_LOOK: Fx = Fx::from_int(2);

/// Declares [`Regiments`] from one list of its per-regiment fields, each
/// with the value a free slot holds, and from that list alone makes the
/// pool, resets a slot, copies one from another world and writes and reads
/// one for a snapshot, and hashes it, so no field can be left out of any
/// of them (review fix R2/T6).
macro_rules! regiment_fields {
    ($($(#[$doc:meta])* $name:ident: $ty:ty = $free:expr,)*) => {
        /// Structure-of-arrays pool of regiments with a fixed capacity.
        ///
        /// Every array is allocated to full capacity up front; spawning and
        /// freeing only flip flags and push/pop a free list that never grows
        /// past capacity. A new per-regiment field goes in the list given to
        /// `regiment_fields!`.
        pub struct Regiments {
            /// Morale tenths added to (or taken from) the ceiling of every
            /// regiment of each party for the rest of the battle, by its
            /// side's champion winning or losing (see [`crate::champions`]).
            /// By party id.
            pub(crate) party_morale: Vec<i16>,
            pub(crate) alive: Vec<bool>,
            pub(crate) generation: Vec<u16>,
            /// Hit points per soldier, `MAX_MODELS` per regiment, by
            /// formation slot (slot `k` is rank `k / files`, file
            /// `k % files`; rank 0 is the front).
            pub(crate) hp: Vec<u16>,
            $($(#[$doc])* pub(crate) $name: Vec<$ty>,)*
            /// Free slot indices, popped from the back. Initialised so slot
            /// 0 is used first.
            pub(crate) free: Vec<u32>,
        }

        impl Regiments {
            pub(crate) fn with_capacity(capacity: u32) -> Regiments {
                let n = capacity as usize;
                Regiments {
                    party_morale: vec![0; PlayerId::MAX as usize + 1],
                    alive: vec![false; n],
                    generation: vec![0; n],
                    hp: vec![0; n * MAX_MODELS as usize],
                    $($name: vec![$free; n],)*
                    free: (0..capacity).rev().collect(),
                }
            }

            /// Slot `i` as a free slot holds it (its handle aside).
            fn reset_slot(&mut self, i: usize) {
                self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize].fill(0);
                $(self.$name[i] = $free;)*
            }

            /// Slot `i`'s fields as `from` holds them (its handle aside).
            fn copy_slot(&mut self, from: &Regiments, i: usize) {
                let m = MAX_MODELS as usize;
                self.hp[i * m..][..m].copy_from_slice(&from.hp[i * m..][..m]);
                $(self.$name[i] = from.$name[i];)*
            }

            /// Slot `i`'s fields (its handle aside), into the state hash:
            /// the soldiers still standing's hit points, then every listed
            /// field.
            pub(crate) fn hash_fields(&self, i: usize, h: &mut StateHasher) {
                let m = MAX_MODELS as usize;
                for hp in &self.hp[i * m..][..self.models[i] as usize] {
                    hp.hash(h);
                }
                $(self.$name[i].hash(h);)*
            }

            /// Slot `i`'s fields (its handle aside), for a snapshot.
            #[cfg_attr(not(test), allow(dead_code, reason = "snapshots of a whole world come with step 6c"))]
            fn write_slot(&self, i: usize, out: &mut Vec<u8>) {
                for hp in &self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize] {
                    hp.write(out);
                }
                $(self.$name[i].write(out);)*
            }

            /// What [`Self::write_slot`] wrote, into slot `i`.
            #[cfg_attr(not(test), allow(dead_code, reason = "snapshots of a whole world come with step 6c"))]
            fn read_slot(&mut self, i: usize, input: &mut &[u8]) -> Option<()> {
                for k in 0..MAX_MODELS as usize {
                    self.hp[i * MAX_MODELS as usize + k] = u16::read(input)?;
                }
                $(self.$name[i] = <$ty>::read(input)?;)*
                Some(())
            }

            /// The listed fields' names, in order.
            #[cfg(test)]
            const FIELDS: &[&str] = &[$(stringify!($name)),*];

            /// Change the `field`th listed field of slot `i` to some other
            /// value.
            #[cfg(test)]
            fn change(&mut self, i: usize, field: usize) {
                let mut k = 0;
                $(
                    if k == field {
                        self.$name[i] = crate::state::other(self.$name[i]);
                    }
                    k += 1;
                )*
                let _ = k;
            }
        }
    };
}

regiment_fields! {
    owner: PlayerId = 0,
    unit: UnitId = 0,
    /// Centre of the block.
    position: FxVec2 = FxVec2::ZERO,
    facing: Angle = Angle::ZERO,
    /// Soldiers per rank.
    files: u16 = 0,
    /// The frontage it was given (soldiers per rank): `files` is narrower
    /// while it squeezes through ground too tight for its block (see
    /// [`Regiments::squeeze`]).
    frontage: u16 = 0,
    /// Ticks until it re-forms by another file, squeezing or widening.
    reform: u8 = 0,
    /// How closely the soldiers stand.
    formation: Formation = Formation::Line,
    /// Soldiers still standing.
    models: u16 = 0,
    /// Full marching speed, tiles per tick.
    speed: Fx = Fx::ZERO,
    /// Turn rate, angle steps per tick.
    turn: u16 = 0,
    target: Option<FxVec2> = None,
    target_facing: Option<Angle> = None,
    /// Speed for the current order, tiles per tick.
    pace: Fx = Fx::ZERO,
    /// Enemy regiment this one was ordered to attack.
    attack_target: Option<RegimentHandle> = None,
    /// The enemy it is fighting in melee this tick (the nearest one in contact).
    engaged: Option<RegimentHandle> = None,
    /// Ticks of charge bonus left.
    charge: u16 = 0,
    /// That charge's bonus, percent: the unit's own, or for braced spears
    /// a charge turned back on its charger (see `Combat::before_move`).
    charge_bonus: u8 = 0,
    strike: Strike = Strike::Turn,
    /// Ticks until a ranged regiment can loose its next volley.
    reload: u16 = 0,
    /// Damage taken but not yet a whole hit point, in 1/256ths.
    damage_carry: u16 = 0,
    /// Soldiers at full strength (the unit's), for casualty fractions.
    full_models: u16 = 0,
    /// Morale in tenths of a point (see [`crate::morale`]), and what it means.
    morale: i16 = 0,
    morale_state: MoraleState = MoraleState::Steady,
    /// Soldiers lost, and flank (bit 0) or rear (bit 1) hits taken, since the
    /// regiment's last morale check.
    losses: u16 = 0,
    shock: u8 = 0,
    /// Hit points lost since the regiment's last morale check, wounds and
    /// deaths alike: what its nerve feels (see [`crate::morale`]).
    bled: u16 = 0,
    /// While routing: ticks spent with no enemy near.
    rout_clock: u16 = 0,
    /// Tick the regiment last broke and ran, plus one (0: never).
    last_rout: u32 = 0,
    /// Ticks spent waiting behind a friendly block in its way (see
    /// [`crate::combat::Combat::friend_in_the_way`]), counted RECHECK at a
    /// time at each look; 0 while marching freely.
    held: u16 = 0,
    /// Ticks before it looks for friends in its way again (see
    /// `Regiments::step_movement`): standing, while it waits behind one;
    /// marching, one after a look, or more while none holding their ground
    /// were near enough to reach sooner.
    clear: u8 = 0,
    /// How deep the block overlaps its friends, in tiles (see
    /// `Combat::crowding`).
    crowd: Fx = Fx::ZERO,
    /// Ticks left until it is back in order (0: in order).
    disorder: u8 = 0,
    /// Ticks it has stepped forward since it last halted or fought, up to
    /// [`crate::combat::RUN_UP_TICKS`]: only a run-up makes a charge.
    run_up: u8 = 0,
    /// Melee damage dealt and taken lately, in 1/256ths of a hit point
    /// weighted by the striker's mass: who is winning the exchange, for
    /// lines that push (see [`Combat::push_lines`]). Both fade by an
    /// eighth each second.
    pressing: u32 = 0,
    pressed: u32 = 0,
    /// How fast it is giving ground, in percent of
    /// [`GIVE_GROUND`](crate::combat::GIVE_GROUND), from the exchange as it
    /// stood at its last blow.
    giving: u8 = 0,
    /// Ticks left of being driven back at full pace by a charge that
    /// ploughed into it (see `combat::ploughs`), whatever the exchange says.
    ploughed: u8 = 0,
    /// The charger that ploughed into it.
    ploughed_by: Option<RegimentHandle> = None,
    /// Spears or pikes set against a charge (see [`Regiments::brace`]):
    /// standing until given another order.
    braced: bool = false,
    /// Ticks left of pulling out of a fight in good order (see
    /// [`Regiments::fall_back`]); 0 when not falling back. It starts at
    /// [`FALL_BACK_TICKS`](crate::combat::FALL_BACK_TICKS) for the tick the
    /// order lands, when its foes still hold it for their parting blow.
    falling_back: u16 = 0,
    /// A haste (see [`crate::powers`]): ticks left (0 for none), and how
    /// many percent faster it reloads, strikes and builds.
    haste_left: u16 = 0,
    haste: u8 = 0,
    /// Ticks left of a march (see [`crate::powers`]) and how many percent
    /// faster it goes.
    march_left: u16 = 0,
    march: u8 = 0,
    /// Ticks left that it can't rout (see [`crate::powers`]).
    steadfast_left: u16 = 0,
    /// Ticks left of a summoned regiment's life (see [`crate::powers`]);
    /// 0 for one that isn't summoned. It never routs while it lasts.
    summoned_left: u16 = 0,
}

impl Regiments {
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
    /// (see [`UnitStats::sanitized`]). `None` when the pool is full or the
    /// owner is past [`MAX_PARTIES`](crate::MAX_PARTIES).
    pub(crate) fn spawn(
        &mut self,
        owner: PlayerId,
        unit: UnitId,
        position: FxVec2,
        facing: Angle,
        stats: &UnitStats,
    ) -> Option<RegimentHandle> {
        if usize::from(owner) >= crate::MAX_PARTIES {
            return None;
        }
        let index = self.free.pop()?;
        let i = index as usize;
        self.reset_slot(i);
        self.alive[i] = true;
        self.owner[i] = owner;
        self.unit[i] = unit;
        self.position[i] = position;
        self.facing[i] = facing;
        self.files[i] = stats.files;
        self.frontage[i] = stats.files;
        self.models[i] = stats.models;
        self.hp[i * MAX_MODELS as usize..][..stats.models as usize].fill(stats.hp_per_model);
        self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
        self.turn[i] = (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u32::from(u16::MAX)) as u16;
        self.pace[i] = self.speed[i];
        self.full_models[i] = stats.models;
        self.morale[i] = morale::fresh(stats);
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
            // Long weapons taken away: nothing left to brace.
            if !stats.reach {
                self.braced[i] = false;
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

    pub(crate) fn despawn(&mut self, h: RegimentHandle) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.alive[i] = false;
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.free.push(h.index);
        true
    }

    /// Make free slot `h.index` the next one [`Regiments::spawn`] fills, with
    /// `h`'s generation, so a regiment copied from another world keeps its
    /// handle. False if that slot is taken or out of range.
    pub(crate) fn claim(&mut self, h: RegimentHandle) -> bool {
        let i = h.index as usize;
        if i >= self.alive.len() || self.alive[i] {
            return false;
        }
        let Some(k) = self.free.iter().rposition(|&f| f == h.index) else {
            return false;
        };
        let index = self.free.remove(k);
        self.free.push(index);
        self.generation[i] = h.generation;
        true
    }

    /// Copy everything about regiment slot `i` of `from` into the same slot
    /// here, which must be free.
    pub(crate) fn copy_from(&mut self, from: &Regiments, i: usize) -> bool {
        if !from.alive[i] || !self.claim(from.handle(i)) {
            return false;
        }
        self.free.pop();
        self.alive[i] = true;
        self.copy_slot(from, i);
        true
    }

    /// The whole pool, for a snapshot: [`Self::read`] gives it back exactly,
    /// free list and handles included.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        (self.alive.len() as u32).write(out);
        for bonus in &self.party_morale[..crate::MAX_PARTIES] {
            bonus.write(out);
        }
        (self.free.len() as u32).write(out);
        for f in &self.free {
            f.write(out);
        }
        for i in 0..self.alive.len() {
            self.alive[i].write(out);
            self.generation[i].write(out);
            if self.alive[i] {
                self.write_slot(i, out);
            }
        }
    }

    /// A pool [`Self::write`] wrote, taken off the front of `input`; `None`
    /// if it is cut short or doesn't hold a pool.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    pub(crate) fn read(input: &mut &[u8]) -> Option<Regiments> {
        let capacity = u32::read(input)?;
        // Every slot takes at least three bytes.
        if capacity as usize > input.len() / 3 {
            return None;
        }
        let mut r = Regiments::with_capacity(capacity);
        for bonus in &mut r.party_morale[..crate::MAX_PARTIES] {
            *bonus = i16::read(input)?;
        }
        let free = u32::read(input)?;
        if free > capacity {
            return None;
        }
        r.free.clear();
        for _ in 0..free {
            let f = u32::read(input)?;
            if f >= capacity {
                return None;
            }
            r.free.push(f);
        }
        for i in 0..capacity as usize {
            r.alive[i] = bool::read(input)?;
            r.generation[i] = u16::read(input)?;
            if r.alive[i] {
                r.read_slot(i, input)?;
                // What the simulation relies on of a live regiment (its
                // unit type is the caller's to check against the table).
                if r.models[i] > MAX_MODELS
                    || r.models[i] > r.full_models[i]
                    || usize::from(r.owner[i]) >= crate::MAX_PARTIES
                {
                    return None;
                }
            }
        }
        // The free list holds every other slot, once.
        let mut listed = vec![false; capacity as usize];
        for &f in &r.free {
            if r.alive[f as usize] || core::mem::replace(&mut listed[f as usize], true) {
                return None;
            }
        }
        (r.free.len() + r.alive.iter().filter(|&&a| a).count() == capacity as usize).then_some(r)
    }

    /// The handle of the regiment in slot `i`.
    pub(crate) fn handle(&self, i: usize) -> RegimentHandle {
        RegimentHandle {
            index: i as u32,
            generation: self.generation[i],
        }
    }

    pub(crate) fn resolve(&self, h: RegimentHandle) -> Option<usize> {
        let i = h.index as usize;
        (i < self.alive.len() && self.alive[i] && self.generation[i] == h.generation).then_some(i)
    }

    /// Regiment `h`'s slot, if it is alive and `player`'s.
    pub(crate) fn owned(&self, h: RegimentHandle, player: PlayerId) -> Option<usize> {
        self.resolve(h).filter(|&i| self.owner[i] == player)
    }

    /// Regiment `h`'s slot, if it is alive, `player`'s and not routing: one
    /// that takes their orders.
    pub(crate) fn standing(&self, h: RegimentHandle, player: PlayerId) -> Option<usize> {
        self.owned(h, player)
            .filter(|&i| !self.morale_state[i].broken())
    }

    /// The live regiment `i` is locked in melee with, if any.
    pub(crate) fn foe(&self, i: usize) -> Option<usize> {
        self.engaged[i].and_then(|h| self.resolve(h))
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
    pub(crate) fn order(&mut self, h: RegimentHandle, order: Order) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.morale_state[i].broken() {
            return false;
        }
        self.target[i] = Some(order.target);
        self.target_facing[i] = order.facing;
        self.attack_target[i] = None;
        self.braced[i] = false;
        self.falling_back[i] = 0;
        self.held[i] = 0;
        self.clear[i] = 0;
        if order.files != 0 {
            self.set_frontage(i, order.files);
        }
        let pace = Fx::from_raw(order.pace.raw() / TICKS_PER_SECOND as i32);
        self.pace[i] = if pace.raw() <= 0 {
            self.speed[i]
        } else {
            pace.min(self.speed[i])
        };
        true
    }

    /// Put a regiment straight down at `position` (deployment): no march,
    /// no order left over. `files` 0 keeps its frontage. Returns its slot.
    pub(crate) fn place(
        &mut self,
        h: RegimentHandle,
        position: FxVec2,
        facing: Angle,
        files: u16,
    ) -> Option<usize> {
        let i = self.resolve(h)?;
        self.position[i] = position;
        self.facing[i] = facing;
        if files != 0 {
            self.set_frontage(i, files);
        }
        self.halt(i);
        Some(i)
    }

    /// Slot `i` forms up `files` soldiers wide (it narrows again at once
    /// if the ground is too tight for that).
    pub(crate) fn set_frontage(&mut self, i: usize, files: u16) {
        let files = files.clamp(1, self.models[i].max(1));
        self.files[i] = files;
        self.frontage[i] = files;
        self.reform[i] = 0;
    }

    /// Cells a route should keep between slot `i`'s centre and blocked
    /// ground: half the larger side of its block at its full frontage,
    /// and half a tile more, between [`nav::CLEARANCE`] and
    /// [`nav::MAX_BERTH`].
    pub(crate) fn berth(&self, units: &[UnitStats], i: usize) -> u8 {
        let stats = &units[self.unit[i] as usize];
        let files = self.frontage[i].max(self.files[i]);
        Self::berth_of(stats, self.formation[i], files, self.models[i])
    }

    /// The berth of a block of `models` soldiers of `stats` in `files`
    /// files in `formation` (see [`Regiments::berth`]).
    pub(crate) fn berth_of(stats: &UnitStats, formation: Formation, files: u16, models: u16) -> u8 {
        let (w, d) = stats.block_size_in(formation, files, models);
        // Half the side in cells is the side in tiles (2 cells a tile),
        // rounded up, plus one cell.
        let cells = w.max(d).ceil_int() + 1;
        cells.clamp(i32::from(nav::CLEARANCE), i32::from(nav::MAX_BERTH)) as u8
    }

    /// Whether slot `i` is standing, not marching (it may be fighting).
    /// What the formation discipline bonus and its readouts go by.
    pub(crate) fn still(&self, i: usize) -> bool {
        self.target[i].is_none()
    }

    /// Whether slot `i` can't rout: steadfast, or a summoned regiment (see
    /// [`crate::powers`]).
    pub(crate) fn cant_rout(&self, i: usize) -> bool {
        self.steadfast_left[i] > 0 || self.summoned_left[i] > 0
    }

    /// Slot `i` narrows into a deeper column where the ground beside it
    /// is too tight for its block (a ford, a pass, a gap between woods on
    /// the edge of the water), a file at a time, looking a little ahead
    /// while it marches; once there is room again it widens back to its
    /// frontage, more slowly, so it doesn't flicker. Only while it marches
    /// or is still narrowed, never in melee or braced.
    fn squeeze(&mut self, i: usize, nav: &Nav, units: &[UnitStats]) {
        let models = self.models[i].max(1);
        let frontage = self.frontage[i].clamp(1, models);
        let marching = self.target[i].is_some();
        if (!marching && self.files[i] == frontage) || !nav.has_map() {
            return;
        }
        if self.reform[i] > 0 {
            self.reform[i] -= 1;
            return;
        }
        let stats = &units[self.unit[i] as usize];
        let (fs, rs) = stats.spacing(self.formation[i]);
        let look = if marching { SQUEEZE_LOOK } else { Fx::ZERO };
        // Open all round for the whole block at full frontage: no need to
        // measure.
        let (w, d) = formation::block_size(frontage, models, fs, rs);
        let reach = (w + d) / 2 + look + SQUEEZE_MARGIN + WIDEN_SPARE;
        let reach_tiles = reach
            .ceil_int()
            .min(i32::from(nav::CLEAR_CAP) / nav::CELLS_PER_TILE);
        let files = self.files[i];
        // The outermost soldiers stand half a spacing in from the block's
        // edge.
        let outer = |f: u16| Fx::from_raw(fs.raw() * i32::from(f - 1) / 2) + SQUEEZE_MARGIN;
        // Room either side of the centre, unless open all round (then the
        // whole frontage fits, however wide: the measure stops at
        // `CLEAR_CAP`).
        let room = (!nav.open_around(self.owner[i], self.position[i], reach_tiles)).then(|| {
            let half = formation::block_size(files, models, fs, rs).1 / 2;
            nav.room_across(
                self.owner[i],
                self.position[i],
                self.facing[i],
                Fx::from_int(reach_tiles),
                &[half + look, half, Fx::ZERO, -half],
            )
        });
        let fits = |f: u16, spare: Fx| room.is_none_or(|room| outer(f) + spare <= room);
        let fit = (1..=frontage)
            .rev()
            .find(|&f| fits(f, Fx::ZERO))
            .unwrap_or(1);
        if fit < files {
            // Halve the gap each step: a sudden narrows closes quickly.
            self.files[i] = files - (files - fit).div_ceil(2);
            self.reform[i] = SQUEEZE_TICKS;
        } else if fit > files && fits(files + 1, WIDEN_SPARE) {
            self.files[i] = files + 1;
            self.reform[i] = WIDEN_TICKS;
        }
    }

    /// Slot `i` drops its orders: no move, no facing to turn to, no enemy.
    pub(crate) fn halt(&mut self, i: usize) {
        self.target[i] = None;
        self.target_facing[i] = None;
        self.attack_target[i] = None;
    }

    /// Slot `i` is out of its fight: no foe, and who was winning forgotten.
    pub(crate) fn leave_fight(&mut self, i: usize) {
        self.engaged[i] = None;
        self.pressing[i] = 0;
        self.pressed[i] = 0;
        self.giving[i] = 0;
        self.ploughed[i] = 0;
        self.ploughed_by[i] = None;
    }

    /// The charger driving slot `i` back after ploughing into it: only
    /// while that lasts, `i` is in a fight and holding together, and the
    /// charger is still fighting it.
    pub(crate) fn plougher(&self, i: usize) -> Option<usize> {
        if self.ploughed[i] == 0 || self.engaged[i].is_none() || self.morale_state[i].broken() {
            return None;
        }
        self.ploughed_by[i]
            .and_then(|h| self.resolve(h))
            .filter(|&k| self.foe(k) == Some(i))
    }

    /// How fast slot `i` is being driven back, in percent of
    /// [`GIVE_GROUND`](crate::combat::GIVE_GROUND): at full pace while a
    /// charge that ploughed into it lasts, else as the exchange says.
    pub(crate) fn driven_pace(&self, i: usize) -> u8 {
        if self.plougher(i).is_some() {
            100
        } else {
            self.giving[i]
        }
    }

    /// Slot `i` is gone from its fight altogether: out of it, and nobody
    /// still locked with it to press after it.
    pub(crate) fn vanish_from_fight(&mut self, i: usize) {
        let h = self.handle(i);
        for k in 0..self.alive.len() {
            if self.engaged[k] == Some(h) {
                self.leave_fight(k);
            }
        }
        self.leave_fight(i);
    }

    /// Order a regiment to attack an enemy regiment: melee regiments close
    /// in and fight, ranged ones move into range and shoot.
    pub(crate) fn attack(&mut self, h: RegimentHandle, target: RegimentHandle) -> bool {
        let (Some(i), Some(t)) = (self.resolve(h), self.resolve(target)) else {
            return false;
        };
        if self.owner[i] == self.owner[t] || self.morale_state[i].broken() {
            return false;
        }
        self.attack_target[i] = Some(target);
        self.target_facing[i] = None;
        self.braced[i] = false;
        self.falling_back[i] = 0;
        self.held[i] = 0;
        self.clear[i] = 0;
        self.pace[i] = self.speed[i];
        true
    }

    /// Order a regiment to pull out of its fight in good order: it backs
    /// away to `to` facing the way it faces now, at half pace, and the
    /// enemies it was fighting each get one parting blow (they strike at
    /// once) but don't follow it. While it falls back nobody in front of it
    /// can lock it in melee; an enemy reaching its flank or rear catches it
    /// (see [`crate::combat::Combat::before_move`]). Routing regiments don't
    /// listen, and one already falling back carries on as it was.
    pub(crate) fn fall_back(&mut self, h: RegimentHandle, to: FxVec2) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.morale_state[i].broken() || self.falling_back[i] > 0 {
            return false;
        }
        for k in 0..self.alive.len() {
            if self.alive[k] && self.engaged[k] == Some(h) {
                self.strike[k] = Strike::Early;
            }
        }
        self.leave_fight(i);
        self.target[i] = Some(to);
        self.target_facing[i] = Some(self.facing[i]);
        self.attack_target[i] = None;
        self.braced[i] = false;
        self.falling_back[i] = crate::combat::FALL_BACK_TICKS;
        self.held[i] = 0;
        self.clear[i] = 0;
        // Backing steps are half steps (see `step_movement`).
        self.pace[i] = self.speed[i];
        true
    }

    /// Set (or lift) a regiment's spears against a charge: it halts where
    /// it stands, and a charge into its front loses its bonus and runs
    /// onto the points instead (see [`crate::combat::Combat::before_move`]).
    /// Only regiments with long weapons ([`UnitStats::reach`]) can brace,
    /// and not while locked in melee, falling back or routing. Any move or
    /// attack order lifts it.
    pub(crate) fn brace(&mut self, h: RegimentHandle, on: bool, units: &[UnitStats]) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if !on {
            return core::mem::take(&mut self.braced[i]);
        }
        if self.morale_state[i].broken()
            || self.engaged[i].is_some()
            || self.falling_back[i] > 0
            || !units[self.unit[i] as usize].reach
        {
            return false;
        }
        self.braced[i] = true;
        self.target[i] = None;
        self.target_facing[i] = None;
        self.attack_target[i] = None;
        self.held[i] = 0;
        true
    }

    /// Take `amount` hit points off the regiment, from the back ranks forward:
    /// the last slot loses hit points first, and each soldier who falls takes
    /// the next one's place in the damage. Returns soldiers lost.
    pub(crate) fn take_damage(&mut self, i: usize, amount: u32) -> u16 {
        self.take_damage_hp(i, amount).0
    }

    /// [`Regiments::take_damage`], also telling the hit points taken (less
    /// than `amount` when the regiment had fewer left).
    pub(crate) fn take_damage_hp(&mut self, i: usize, mut amount: u32) -> (u16, u32) {
        let asked = amount;
        let before = self.models[i];
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        let mut bled = 0u16;
        while amount > 0 && self.models[i] > 0 {
            let k = self.models[i] as usize - 1;
            if u32::from(hp[k]) > amount {
                hp[k] -= amount as u16;
                bled = bled.saturating_add(amount as u16);
                amount = 0;
            } else {
                amount -= u32::from(hp[k]);
                bled = bled.saturating_add(hp[k]);
                hp[k] = 0;
                self.models[i] -= 1;
            }
        }
        let lost = before - self.models[i];
        self.bleed(i, bled, lost);
        (lost, asked - amount)
    }

    /// Take `damage` hit points off soldier `k` of regiment `i`. Each time
    /// that kills the man in slot `k`, the soldier in the last slot steps up
    /// into his place and takes what the blow had to spare. Returns soldiers
    /// lost.
    pub(crate) fn wound(&mut self, i: usize, k: u16, mut damage: u16) -> u16 {
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        let k = k as usize;
        let mut lost = 0;
        let mut bled = 0;
        while damage > 0 && k < self.models[i] as usize {
            if hp[k] > damage {
                hp[k] -= damage;
                bled += damage;
                break;
            }
            damage -= hp[k];
            bled += hp[k];
            let last = self.models[i] as usize - 1;
            hp[k] = hp[last];
            hp[last] = 0;
            self.models[i] -= 1;
            lost += 1;
        }
        self.bleed(i, bled, lost);
        lost
    }

    /// Books `bled` hit points and `lost` soldiers taken off regiment `i`.
    fn bleed(&mut self, i: usize, bled: u16, lost: u16) {
        self.files[i] = self.files[i].min(self.models[i]).max(1);
        self.bled[i] = self.bled[i].saturating_add(bled);
        self.losses[i] = self.losses[i].saturating_add(lost);
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
            frontage: self.frontage[i].clamp(1, self.models[i].max(1)),
            formation: self.formation[i],
            models: self.models[i],
            moving: self.target[i].is_some() || self.target_facing[i].is_some(),
            still: self.still(i),
            engaged: self.engaged[i].is_some(),
            charge: self.charge[i],
            morale: self.morale_state[i],
            morale_points: (self.morale[i].max(0) / morale::SCALE) as u8,
            disorder: self.disorder[i],
            giving_ground: self.driven_pace(i) > 0 && self.engaged[i].is_some(),
            ploughed: if self.plougher(i).is_some() {
                self.ploughed[i]
            } else {
                0
            },
            ploughed_by: self.plougher(i).map(|k| self.handle(k)),
            braced: self.braced[i],
            falling_back: self.falling_back[i] > 0,
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
            // Falling back ends when time runs out, wherever it got to.
            if self.falling_back[i] > 0 {
                self.falling_back[i] -= 1;
                if self.falling_back[i] == 0 {
                    self.target[i] = None;
                    self.target_facing[i] = None;
                }
            }
            // Halting, fighting or backing away ends a run-up.
            if self.target[i].is_none() || self.engaged[i].is_some() || self.falling_back[i] > 0 {
                self.run_up[i] = 0;
            }
            if self.target[i].is_none() {
                self.held[i] = 0;
                self.clear[i] = 0;
            }
            if self.engaged[i].is_none() && !self.braced[i] {
                self.squeeze(i, nav, units);
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
                // On a march (see [`crate::powers`]), further.
                let pace = self.marched(i, pace);
                // Falling back, it backs away all the way, facing its foe.
                let shuffle = ((last && dist <= SHUFFLE_DISTANCE) || self.falling_back[i] > 0)
                    && self.target_facing[i].is_some();
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
                let dir = if dist.raw() > 0 {
                    FxVec2::new(delta.x / dist, delta.y / dist)
                } else {
                    FxVec2::ZERO
                };
                let mass = units[self.unit[i] as usize].mass;
                let step = terrain.going(step, self.position[i], dir, mass);
                let step = crate::combat::pushing(step, self.crowd[i]);
                debug_assert!(step <= combat.max_step(), "a step outruns the crowd check");
                let here = self.position[i];
                let arriving = dist <= step;
                let next = if arriving { point } else { here + dir * step };
                // A friendly block holding its ground in the way, where the
                // ground leaves no room beside it (a ford, a pass): wait
                // behind it. In the open, blocks still pass through.
                // Marching, it looks at most every other tick (`clear`);
                // waiting, every RECHECK ticks, standing in between.
                if self.clear[i] > 0 {
                    self.clear[i] -= 1;
                    if (1..PATIENCE).contains(&self.held[i]) {
                        continue;
                    }
                } else if !nav.open_around(self.owner[i], here, OPEN_GROUND) {
                    let found = combat.friend_in_the_way(self, units, stances, i, next, dir);
                    // Following a friend marching on ahead (its route goes
                    // where this one's does, room beside it or not) uses
                    // up no patience: the queue is moving.
                    let following = |j: usize| {
                        self.target[j].is_some()
                            && self.engaged[j].is_none()
                            && !(1..PATIENCE).contains(&self.held[j])
                    };
                    match found {
                        Ok(j) if following(j) => {
                            self.held[i] = self.held[i].max(1);
                            self.clear[i] = RECHECK - 1;
                            continue;
                        }
                        Ok(j) if !combat.room_beside(self, units, nav, i, j, dir) => {
                            // Patience runs for the ticks until it looks
                            // again; once it has run out it looks every
                            // tick, standing only while it must.
                            self.held[i] = self.held[i].saturating_add(u16::from(RECHECK));
                            if self.held[i] < PATIENCE {
                                self.clear[i] = RECHECK - 1;
                            }
                            continue;
                        }
                        Ok(_) => self.clear[i] = 1,
                        Err(gap) => {
                            let ticks = gap.raw() / step.raw().max(1);
                            self.clear[i] = ticks.clamp(1, i32::from(CLEAR_TICKS)) as u8;
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
                        self.falling_back[i] = 0;
                    } else {
                        nav.reached(i);
                    }
                } else if let Some(next) = nav.slide(self.owner[i], here, next) {
                    self.position[i] = next;
                } else {
                    nav.lost(i);
                    continue;
                }
                // A run-up is made on ground that bears a charge: wading or
                // slogging through mud starts it again.
                if !terrain.bears_charge(self.position[i]) {
                    self.run_up[i] = 0;
                } else if self.falling_back[i] == 0 {
                    self.run_up[i] = (self.run_up[i] + 1).min(crate::combat::RUN_UP_TICKS);
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
        // No party past MAX_PARTIES ever gets a bonus.
        for bonus in &self.party_morale[..crate::MAX_PARTIES] {
            bonus.hash(h);
        }
        h.u32(self.len() as u32);
        for i in 0..self.alive.len() {
            h.u8(u8::from(self.alive[i]));
            h.u16(self.generation[i]);
            if self.alive[i] {
                self.hash_fields(i, h);
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
    /// Soldiers per rank it was given: more than `files` while it is
    /// squeezed into a column through tight ground.
    pub frontage: u16,
    /// How closely the soldiers stand.
    pub formation: Formation,
    /// Soldiers still standing.
    pub models: u16,
    /// Walking or turning to an ordered facing.
    pub moving: bool,
    /// Not marching: standing, turning or fighting. Formation discipline
    /// goes by this, not `moving` (see [`Regiments::still`]).
    pub still: bool,
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
    /// Losing its melee and being driven back (see
    /// [`Combat::push_lines`](crate::combat::Combat::push_lines)), or
    /// pinned where it can't give ground.
    pub giving_ground: bool,
    /// Spears or pikes set against a charge (see [`Regiments::brace`]).
    pub braced: bool,
    /// Pulling out of a fight in good order (see [`Regiments::fall_back`]).
    pub falling_back: bool,
    /// Ticks left of being driven back by a charge that ploughed into its
    /// ranks, up to [`PLOUGH_TICKS`](crate::combat::PLOUGH_TICKS).
    pub ploughed: u8,
    /// The charger that ploughed in, while it does.
    pub ploughed_by: Option<RegimentHandle>,
}

impl RegimentView {
    pub fn ranks(&self) -> u16 {
        formation::ranks(self.files, self.models)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::scenario::{DRILL_UNITS, Drill, ribs_map};
    use crate::{Capacities, Ground, MatchSetup, World};

    /// A world mid-battle, blocks pushing in melee.
    fn mid_melee() -> (World, Drill) {
        let nav = ribs_map(256);
        let mut world = World::new(&MatchSetup {
            seed: 31,
            capacities: Capacities::default(),
            units: &DRILL_UNITS,
            map_tiles: 256,
            nav: &nav,
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
        });
        let mut drill = Drill::new(7);
        drill.regiments_per_player = 20;
        drill.unit_of = |p, _, _| [0, 1, 3, 0][p as usize % 4];
        let mut commands = Vec::new();
        while !world.regiments.pressing.iter().any(|&p| p != 0) {
            assert!(world.tick_count() < 900, "no block ever pushed");
            drill.commands_for(&world, &mut commands);
            world.tick(&commands);
        }
        (world, drill)
    }

    fn hash(r: &Regiments) -> u64 {
        let mut h = StateHasher::new();
        r.hash_into(&mut h);
        h.finish()
    }

    fn copy(r: &Regiments) -> Regiments {
        let mut bytes = Vec::new();
        r.write(&mut bytes);
        let mut input = &bytes[..];
        let back = Regiments::read(&mut input).unwrap();
        assert!(input.is_empty());
        back
    }

    #[test]
    fn every_field_reaches_the_state_hash() {
        let (world, _) = mid_melee();
        let regs = &world.regiments;
        let i = (0..regs.capacity()).find(|&i| regs.alive[i]).unwrap();
        let before = hash(regs);
        for (k, name) in Regiments::FIELDS.iter().enumerate() {
            let mut changed = copy(regs);
            changed.change(i, k);
            assert_ne!(hash(&changed), before, "{name} isn't hashed");
        }
        // And the fields outside the list.
        let mut changed = copy(regs);
        changed.hp[i * MAX_MODELS as usize] ^= 1;
        assert_ne!(hash(&changed), before, "hp isn't hashed");
        let mut changed = copy(regs);
        changed.generation[i] ^= 1;
        assert_ne!(hash(&changed), before, "generation isn't hashed");
        let mut changed = copy(regs);
        changed.party_morale[crate::MAX_PARTIES - 1] = 5;
        assert_ne!(hash(&changed), before, "party morale isn't hashed");
        let mut changed = copy(regs);
        changed.despawn(changed.handle(i));
        assert_ne!(hash(&changed), before, "who is alive isn't hashed");
    }

    #[test]
    fn a_pool_that_breaks_the_rules_is_refused() {
        let (world, _) = mid_melee();
        let regs = &world.regiments;
        let live = (0..regs.capacity()).find(|&i| regs.alive[i]).unwrap() as u32;
        let refused = |edit: &dyn Fn(&mut Regiments)| {
            let mut bad = copy(regs);
            edit(&mut bad);
            let mut bytes = Vec::new();
            bad.write(&mut bytes);
            Regiments::read(&mut &bytes[..]).is_none()
        };
        assert!(refused(&|r| r.free.push(live)), "a live slot listed free");
        assert!(
            refused(&|r| {
                let f = r.free[0];
                r.free.push(f);
            }),
            "a slot listed free twice"
        );
        assert!(
            refused(&|r| {
                r.free.pop();
            }),
            "a free slot left off the list"
        );
        assert!(refused(&|r| r.models[live as usize] = MAX_MODELS + 1));
        assert!(refused(
            &|r| r.owner[live as usize] = crate::MAX_PARTIES as u8
        ));
    }

    #[test]
    fn a_pool_reads_back_exactly_and_plays_on_alike() {
        let (mut world, _) = mid_melee();
        let mut bytes = Vec::new();
        world.regiments.write(&mut bytes);
        let mut other = World::new(&MatchSetup {
            seed: 31,
            capacities: Capacities::default(),
            units: &DRILL_UNITS,
            map_tiles: 256,
            nav: &ribs_map(256),
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
        });
        other.regiments = copy(&world.regiments);
        let mut again = Vec::new();
        other.regiments.write(&mut again);
        assert_eq!(again, bytes);
        assert_eq!(hash(&other.regiments), hash(&world.regiments));
        // Cut short anywhere, it reads as nothing.
        for cut in [0, 3, bytes.len() / 2, bytes.len() - 1] {
            assert!(
                Regiments::read(&mut &bytes[..cut]).is_none(),
                "cut at {cut}"
            );
        }
        // The free list came back too: both pools fill the same slot next.
        let stats = DRILL_UNITS[0].sanitized();
        assert_eq!(
            world
                .regiments
                .spawn(0, 0, FxVec2::ZERO, Angle::ZERO, &stats),
            other
                .regiments
                .spawn(0, 0, FxVec2::ZERO, Angle::ZERO, &stats)
        );
    }
}
