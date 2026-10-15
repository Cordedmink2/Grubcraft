//! What a mind can see: the simulation's public views, copied once per tick
//! into plain floating-point records the rest of the AI reads.
//!
//! A mind never reads simulation internals, only what a player could see on
//! screen: each regiment's position, facing, frontage, strength, whether it
//! is moving or fighting, and its morale as its banner shows it. On a map
//! with woods and tall grass (docs/20-ambushes.md) a mind sees only what its
//! side sees ([`Perception::capture_for`]): enemies hidden in cover are left
//! out, and its own regiments say whether they are hidden.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::{
    Formation, MoraleState, PlayerId, RegimentHandle, Stances, TICKS_PER_SECOND, UnitId, UnitStats,
    World,
};
use glam::Vec2;

/// Simulation ticks per second, as a float.
pub const TICKS: f32 = TICKS_PER_SECOND as f32;

/// Marching speed from which a melee regiment counts as fast shock troops
/// (cavalry, or fast light infantry like the Brood's Skitterlings), in tiles
/// per second. Line infantry marches at about 3.
pub const SHOCK_SPEED: f32 = 3.6;

/// How a regiment fights, as the AI sorts them for roles (docs/18-ai.md,
/// "The battle frame"). More classes (heavy, air, artillery, monster, hero)
/// arrive with the rosters that need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Melee infantry that holds a line.
    Line,
    /// Fast melee troops: flankers, chasers, the hammer.
    Shock,
    /// Shoots.
    Ranged,
}

impl Class {
    pub fn of(stats: &UnitStats) -> Class {
        if stats.range.raw() > 0 {
            Class::Ranged
        } else if fx(stats.speed) >= SHOCK_SPEED {
            Class::Shock
        } else {
            Class::Line
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Class::Line => "line",
            Class::Shock => "shock",
            Class::Ranged => "ranged",
        }
    }
}

/// A regiment type as the AI reads it.
#[derive(Clone, Copy, Debug)]
pub struct UnitInfo {
    pub stats: UnitStats,
    pub class: Class,
    /// Tiles per second.
    pub speed: f32,
    /// Shooting range in tiles; zero for melee regiments.
    pub range: f32,
    /// Never routs (the Concord's undead).
    pub stalwart: bool,
}

impl UnitInfo {
    pub fn of(stats: &UnitStats) -> UnitInfo {
        UnitInfo {
            stats: *stats,
            class: Class::of(stats),
            speed: fx(stats.speed),
            range: fx(stats.range),
            stalwart: stats.morale == 0,
        }
    }

    /// Seconds between volleys.
    pub fn reload_seconds(&self) -> f32 {
        f32::from(self.stats.reload_ticks.max(1)) / TICKS_PER_SECOND as f32
    }
}

/// One regiment as a mind sees it.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub handle: RegimentHandle,
    pub owner: PlayerId,
    pub unit: UnitId,
    /// Centre of the block, in tiles.
    pub pos: Vec2,
    /// Unit vector the block faces.
    pub facing: Vec2,
    /// Width across the front and depth front to back, in tiles.
    pub width: f32,
    pub depth: f32,
    pub files: u16,
    pub formation: Formation,
    /// Soldiers standing, and at full strength.
    pub models: u16,
    pub full: u16,
    pub moving: bool,
    pub engaged: bool,
    pub morale: MoraleState,
    /// Morale in points, 0 to 100 (meaningless for units that never rout).
    pub morale_points: u8,
    /// No enemy sees it (in cover, unspotted). Always false for enemies a
    /// mind sees, of course.
    pub hidden: bool,
}

impl Seen {
    /// Running or shattered: out of the fight.
    pub fn broken(&self) -> bool {
        self.morale.broken()
    }

    /// Shaken: wavering, or worse.
    pub fn shaken(&self) -> bool {
        self.morale != MoraleState::Steady
    }

    /// Fraction of full strength still standing.
    pub fn strength(&self) -> f32 {
        f32::from(self.models) / f32::from(self.full.max(1))
    }

    /// Radius of a circle that holds the block.
    pub fn radius(&self) -> f32 {
        0.5 * (self.width + self.depth)
    }

    /// How far this block is from `other` along the axis that separates
    /// them best, in tiles: the gap between their edges when apart, and
    /// less than zero by as much as they overlap (as the simulation
    /// measures crowding).
    pub fn clearance(&self, other: &Seen) -> f32 {
        let d = other.pos - self.pos;
        [
            self.facing,
            right_of(self.facing),
            other.facing,
            right_of(other.facing),
        ]
        .iter()
        .map(|&axis| d.dot(axis).abs() - self.half_extent(axis) - other.half_extent(axis))
        .fold(f32::MIN, f32::max)
    }

    /// Half this block's length along `axis` (a unit vector), in tiles.
    pub fn half_extent(&self, axis: Vec2) -> f32 {
        0.5 * self.width * right_of(self.facing).dot(axis).abs()
            + 0.5 * self.depth * self.facing.dot(axis).abs()
    }

    /// How deep this block and `other` overlap, in tiles (zero when apart).
    pub fn overlap(&self, other: &Seen) -> f32 {
        (-self.clearance(other)).max(0.0)
    }

    /// Which side of this block `point` is on, as the combat rules judge it.
    pub fn side_facing(&self, point: Vec2) -> Side {
        let d = point - self.pos;
        let along = d.dot(self.facing);
        let across = d.dot(right_of(self.facing));
        if along.abs() * self.width * 0.5 >= across.abs() * self.depth * 0.5 {
            if along >= 0.0 {
                Side::Front
            } else {
                Side::Rear
            }
        } else {
            Side::Flank
        }
    }
}

/// Which side of a block something is on (mirrors `gc_sim::combat::Side`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Flank,
    Rear,
}

/// Everything the minds of a match read, captured from the world once per
/// tick and shared by all of them.
#[derive(Clone, Debug, Default)]
pub struct Perception {
    pub tick: u32,
    pub units: Vec<UnitInfo>,
    /// Live regiments in pool order.
    pub regiments: Vec<Seen>,
    pub stances: Stances,
    /// Index into `regiments` by pool index, for handle lookups.
    by_index: Vec<u32>,
    /// Every enemy seen lately, as last seen, and the tick it was.
    memory: Vec<(Seen, u32)>,
}

/// How long a mind remembers an enemy it has lost sight of, in ticks.
pub const REMEMBER_TICKS: u32 = 60 * TICKS_PER_SECOND;

const NONE: u32 = u32::MAX;

impl Perception {
    pub fn new() -> Perception {
        Perception::default()
    }

    /// Copy the whole of `world` now, every regiment whether anyone sees it
    /// or not. Reuses the buffers, so after the first call it allocates
    /// nothing.
    pub fn capture(&mut self, world: &World) {
        self.capture_seen_by(world, None);
    }

    /// Copy what `party`'s side sees of `world` now: the same as
    /// [`Perception::capture`] but without the enemies hidden from it.
    pub fn capture_for(&mut self, world: &World, party: PlayerId) {
        self.capture_seen_by(world, Some(party));
    }

    fn capture_seen_by(&mut self, world: &World, party: Option<PlayerId>) {
        self.tick = world.tick_count();
        if self.units.len() != world.units().len()
            || self
                .units
                .iter()
                .zip(world.units())
                .any(|(a, b)| a.stats != *b)
        {
            self.units.clear();
            self.units.extend(world.units().iter().map(UnitInfo::of));
        }
        if self.stances != *world.stances() {
            self.stances = world.stances().clone();
        }
        self.regiments.clear();
        self.by_index.clear();
        self.by_index.resize(world.regiments.capacity(), NONE);
        for r in world.regiments.iter() {
            if party.is_some_and(|p| !world.sees(p, r.handle)) {
                continue;
            }
            let stats = &self.units[r.unit as usize].stats;
            let (w, d) = stats.block_size_in(r.formation, r.files, r.models);
            self.by_index[r.handle.index as usize] = self.regiments.len() as u32;
            self.regiments.push(Seen {
                handle: r.handle,
                owner: r.owner,
                unit: r.unit,
                pos: v2(r.position),
                facing: angle_vec(r.facing),
                width: fx(w),
                depth: fx(d),
                files: r.files,
                formation: r.formation,
                models: r.models,
                full: stats.models,
                moving: r.moving,
                engaged: r.engaged,
                morale: r.morale,
                morale_points: r.morale_points,
                hidden: world.hidden(r.handle),
            });
        }
        match party {
            Some(p) => self.remember(world, p),
            None => self.memory.clear(),
        }
    }

    /// Enemies seen in the last [`REMEMBER_TICKS`] but out of sight now,
    /// as they were last seen ([`Perception::capture_for`] only).
    pub fn remembered(&self) -> impl Iterator<Item = &Seen> {
        let now = self.tick;
        self.memory
            .iter()
            .filter(move |(_, seen)| *seen != now)
            .map(|(m, _)| m)
    }

    /// Note the enemies `party` sees now. One unseen for
    /// [`REMEMBER_TICKS`], or gone from the field, is forgotten.
    fn remember(&mut self, world: &World, party: PlayerId) {
        let now = self.tick;
        for r in &self.regiments {
            if !self.stances.hostile(party, r.owner) {
                continue;
            }
            match self.memory.iter_mut().find(|(m, _)| m.handle == r.handle) {
                Some(m) => *m = (*r, now),
                None => self.memory.push((*r, now)),
            }
        }
        self.memory.retain(|(m, seen)| {
            *seen <= now && now - seen <= REMEMBER_TICKS && world.regiments.get(m.handle).is_some()
        });
    }

    /// The regiment a handle refers to, if it is still on the field.
    pub fn get(&self, h: RegimentHandle) -> Option<&Seen> {
        let k = *self.by_index.get(h.index as usize)?;
        let r = self.regiments.get(k as usize)?;
        (r.handle == h).then_some(r)
    }

    pub fn unit(&self, r: &Seen) -> &UnitInfo {
        &self.units[r.unit as usize]
    }

    /// How fast `r` marches in its formation, tiles per second.
    pub fn pace(&self, r: &Seen) -> f32 {
        self.unit(r).speed * r.formation.pace_percent() as f32 / 100.0
    }

    pub fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        self.stances.hostile(a, b)
    }
}

/// Tiles from a fixed-point value.
pub fn fx(v: Fx) -> f32 {
    v.raw() as f32 / 65_536.0
}

pub fn v2(v: FxVec2) -> Vec2 {
    Vec2::new(fx(v.x), fx(v.y))
}

/// Back to the simulation's fixed point, rounding to the nearest step.
pub fn to_fx(v: f32) -> Fx {
    Fx::from_raw((v * 65_536.0).round() as i32)
}

pub fn to_fx_vec(v: Vec2) -> FxVec2 {
    FxVec2::new(to_fx(v.x), to_fx(v.y))
}

pub fn angle_vec(a: Angle) -> Vec2 {
    v2(a.to_vec())
}

/// The simulation's angle for a direction (integer, so the same everywhere).
pub fn vec_angle(v: Vec2) -> Angle {
    Angle::of_vec(to_fx_vec(v))
}

/// The direction to the right of `forward` (the sim's +x when facing -y).
pub fn right_of(forward: Vec2) -> Vec2 {
    Vec2::new(-forward.y, forward.x)
}
