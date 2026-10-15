//! Melee and ranged combat (docs/05-combat.md, docs/10-architecture.md).
//!
//! Each tick, in this order:
//! 1. [`Combat::before_move`] finds which enemy blocks touch (oriented
//!    rectangles plus a little reach). A regiment in contact is locked in
//!    melee with the nearest one; contact made at speed starts a charge.
//!    Regiments ordered to attack chase their target.
//! 2. [`Combat::push_lines`]: a regiment losing its melee gives ground and
//!    the enemies fighting it follow; one with nowhere to go is pinned.
//! 3. The regiments move (`Regiments::step_movement`).
//! 4. [`Combat::after_move`] resolves melee blows, looses and lands volleys,
//!    and removes regiments with no soldiers left.
//!
//! Every regiment strikes once a second, staggered by pool index so the work
//! spreads over the ticks. Damage is the expected value of the blows with a
//! ±25% roll from the combat random stream, carried in 1/256ths of a hit point
//! so small hits add up.

use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;

use gc_fixed::{Angle, Fx, FxVec2, Pcg32};

use crate::morale::{MoraleState, SHOCK_FLANK, SHOCK_PINNED, SHOCK_REAR, WAVERING_DAMAGE_PERCENT};
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments, Strike, hash_handle};
use crate::terrain::Terrain;
use crate::vision::{OFF_GUARD_DAMAGE_PERCENT, Vision};
use crate::{PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitStats, formation};

/// Ticks between a regiment's melee blows.
pub const STRIKE_INTERVAL: u32 = TICKS_PER_SECOND;
/// How far apart two blocks' edges can be and still fight, in tiles.
pub const REACH: Fx = Fx::from_ratio(1, 2);
/// How long a charge bonus lasts after contact, fading to nothing.
pub const CHARGE_TICKS: u16 = 3 * TICKS_PER_SECOND as u16;
/// Arrow flight speed, tiles per tick (20 tiles per second).
pub const MISSILE_SPEED: Fx = Fx::ONE;
/// Volleys in flight at once, across all players.
pub const MAX_VOLLEYS: usize = 2048;
/// Ground covered per soldier in a "normal" block (0.8 × 0.8 tiles); denser
/// blocks take more arrow hits, looser ones fewer.
const REFERENCE_AREA_PER_SOLDIER: i64 = (Fx::ONE.raw() as i64 * 64) / 100;

/// A marching regiment stops short of a friendly block it would overlap by
/// more than this, in tiles (see [`Combat::friend_in_the_way`]).
const CROWDING: Fx = Fx::from_ratio(1, 4);
/// A regiment overlapping a friendly block by more than this, in tiles, is
/// pushing through it (or being pushed through): it slows and falls into
/// disorder. Neighbours in a line brushing or wheeling into each other
/// overlap less than this.
pub const PUSHING: Fx = Fx::ONE;
/// A regiment out of order takes this long to re-form once clear (3 s).
pub const DISORDER_TICKS: u8 = 3 * TICKS_PER_SECOND as u8;
/// At full disorder a regiment deals this much less melee damage, and
/// takes this much more, in percent.
pub const DISORDER_PERCENT: u64 = 20;
/// Each regiment measures how deep it overlaps its friends every this many
/// ticks (staggered by index; 0.2 s, short against the 3 s it takes to
/// re-form).
const CROWD_EVERY: u32 = 4;
/// Pushing into friends slows a regiment to this share of its pace, in
/// percent, once it is [`PUSH_RAMP`] past [`CROWDING`] deep.
const PUSH_PERCENT: i64 = 40;
const PUSH_RAMP: Fx = Fx::ONE;
/// Overlap past which neither the slowdown nor the disorder grows: the
/// crowd check stops looking once it finds this much, and stores no more.
const SATURATED: Fx = Fx::from_raw(CROWDING.raw() + PUSH_RAMP.raw());

/// `step` slowed for pushing `crowd` tiles deep into friendly blocks:
/// untouched up to [`CROWDING`], then down to [`PUSH_PERCENT`] of it.
pub(crate) fn pushing(step: Fx, crowd: Fx) -> Fx {
    if crowd <= CROWDING {
        return step;
    }
    let deep = i64::from((crowd - CROWDING).min(PUSH_RAMP).raw());
    let percent = 100 - (100 - PUSH_PERCENT) * deep / i64::from(PUSH_RAMP.raw());
    Fx::from_raw((i64::from(step.raw()) * percent / 100) as i32).max(Fx::from_raw(1))
}
/// It waits this long behind a friend that is only standing or waiting,
/// then walks through, so no order is stuck for good. Friends locked in
/// melee are never walked through.
pub const PATIENCE: u16 = 3 * TICKS_PER_SECOND as u16;
/// How far past touching distance the crowd check looks, so a regiment
/// with no friend that near can march a few ticks without looking again.
const LOOK_AHEAD: Fx = Fx::from_int(2);

/// Lines that push (docs/05-combat.md): a regiment losing the melee
/// exchange gives ground at up to this many tiles a tick (0.4 a second),
/// and the enemies fighting it follow.
pub const GIVE_GROUND: Fx = Fx::from_ratio(1, 50);
/// A regiment losing the exchange by up to this share of the blows
/// traded, in percent (weighted by mass), holds its ground; losing by
/// [`DRIVEN_AT`] or more it gives ground at full pace.
const HOLDS_WITHIN: i64 = 20;
const DRIVEN_AT: i64 = 60;
/// How many regiments down a chain of fights a regiment looks for one
/// giving ground to follow (see [`Combat::follow`]).
const FOLLOW_HOPS: usize = 4;

/// How fast regiment `i` should be driven back, in percent of
/// [`GIVE_GROUND`], by the exchange so far: nothing while it holds its own
/// (or until both sides have struck, when nobody knows who is winning), all
/// of it once it is losing badly.
fn driven(regs: &Regiments, i: usize) -> u8 {
    let (dealt, taken) = (i64::from(regs.pressing[i]), i64::from(regs.pressed[i]));
    if regs.engaged[i].is_none() || dealt == 0 || taken == 0 {
        return 0;
    }
    let losing_by = 100 * (taken - dealt) / (taken + dealt);
    ((losing_by - HOLDS_WITHIN) * 100 / (DRIVEN_AT - HOLDS_WITHIN)).clamp(0, 100) as u8
}

/// Spatial hash cell edge, in tiles, and bucket count (a power of two).
const CELL_TILES: i32 = 16;
const BUCKETS: usize = 1024;
const EMPTY: u32 = u32::MAX;
/// The grid keeps each owner's regiments apart (by `owner % OWNER_SLOTS`) so a
/// search for enemies skips crowds of its own regiments, and a search for
/// friends only visits its own and its allies'.
const OWNER_SLOTS: usize = 8;
/// Grid cells one regiment may occupy before it is checked by everyone instead.
const MAX_CELLS_PER_REGIMENT: usize = 9;
/// Every owner slot, for searches that look at friends and enemies alike.
const ALL_SLOTS: u32 = (1 << OWNER_SLOTS) - 1;

/// Whether a point of a regiment's rear edge, stepping back to `behind`,
/// meets the end of the ground: blocked ground, the map's edge, or water
/// behind a regiment standing on dry land (a river bank at its back).
fn ground_ends(
    terrain: &Terrain,
    nav: &Nav,
    map_tiles: u32,
    centre: FxVec2,
    behind: FxVec2,
) -> bool {
    let edge = Fx::from_int(map_tiles as i32);
    let off_map = map_tiles > 0
        && (behind.x < Fx::ZERO || behind.y < Fx::ZERO || behind.x > edge || behind.y > edge);
    off_map || nav.blocked(behind) || (terrain.wet(behind) && !terrain.wet(centre))
}

/// Which side of a regiment something is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Flank,
    Rear,
}

impl Side {
    /// Damage multiplier, percent, for blows landing on this side.
    pub fn damage_percent(self) -> u64 {
        match self {
            Side::Front => 100,
            Side::Flank => 140,
            Side::Rear => 180,
        }
    }
}

/// A regiment's footprint as an oriented rectangle.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Block {
    pub(crate) centre: FxVec2,
    pub(crate) forward: FxVec2,
    pub(crate) right: FxVec2,
    pub(crate) half_width: Fx,
    pub(crate) half_depth: Fx,
}

impl Block {
    pub(crate) fn of(regs: &Regiments, units: &[UnitStats], i: usize) -> Block {
        Block::at(regs, units, i, regs.position[i])
    }

    /// Regiment `i`'s block if its centre were at `centre`.
    fn at(regs: &Regiments, units: &[UnitStats], i: usize, centre: FxVec2) -> Block {
        let stats = &units[regs.unit[i] as usize];
        let (w, d) = stats.block_size_in(regs.formation[i], regs.files[i], regs.models[i]);
        let forward = regs.facing[i].to_vec();
        Block {
            centre,
            forward,
            right: FxVec2::new(-forward.y, forward.x),
            half_width: Fx::from_raw(w.raw() / 2),
            half_depth: Fx::from_raw(d.raw() / 2),
        }
    }

    /// Radius of a circle that holds the block.
    pub(crate) fn radius(&self) -> Fx {
        self.half_width + self.half_depth
    }

    fn extent_along(&self, axis: FxVec2) -> Fx {
        self.half_width * self.right.dot(axis).abs()
            + self.half_depth * self.forward.dot(axis).abs()
    }

    /// Separating-axis test with `gap` of slack between the edges.
    pub(crate) fn touches(&self, other: &Block, gap: Fx) -> bool {
        let d = other.centre - self.centre;
        [self.forward, self.right, other.forward, other.right]
            .iter()
            .all(|&axis| {
                d.dot(axis).abs() <= self.extent_along(axis) + other.extent_along(axis) + gap
            })
    }

    /// How deep the two blocks overlap: the least they would have to move
    /// apart along any of their edges' directions to stop overlapping
    /// (zero when apart).
    fn overlap(&self, other: &Block) -> Fx {
        let d = other.centre - self.centre;
        [self.forward, self.right, other.forward, other.right]
            .iter()
            .map(|&axis| self.extent_along(axis) + other.extent_along(axis) - d.dot(axis).abs())
            .min()
            .unwrap_or(Fx::ZERO)
            .max(Fx::ZERO)
    }

    /// The side of this block that faces `point`.
    fn side_facing(&self, point: FxVec2) -> Side {
        let d = point - self.centre;
        let along = d.dot(self.forward);
        let across = d.dot(self.right);
        // Compare |along| / half_depth with |across| / half_width without dividing.
        if (along.abs() * self.half_width).raw() >= (across.abs() * self.half_depth).raw() {
            if along.raw() >= 0 {
                Side::Front
            } else {
                Side::Rear
            }
        } else {
            Side::Flank
        }
    }
}

/// Combat state that isn't per regiment: the volleys in flight, and scratch
/// space for the spatial hash. All of it is allocated when the match starts.
pub struct Combat {
    /// Spatial hash: first entry per bucket, then a linked list of entries.
    /// A regiment has an entry in every cell its footprint (plus reach)
    /// covers, up to `MAX_CELLS_PER_REGIMENT`; bigger ones go in `big`.
    head: Vec<u32>,
    entry_next: Vec<u32>,
    entry_regiment: Vec<u32>,
    entries: usize,
    big: Vec<u32>,
    /// Every live regiment's footprint at the start of this tick.
    blocks: Vec<Block>,
    radius: Vec<Fx>,
    /// The biggest of `radius` this tick.
    max_radius: Fx,
    /// The furthest any regiment can step this tick (its speed, and never
    /// less than [`GIVE_GROUND`], the most a line that pushes moves it; a
    /// pushed regiment is in melee, so it doesn't also march). The grid is
    /// filled at the start of the tick, so searches during movement add
    /// this much slack.
    max_step: Fx,
    /// Per grid bucket and owner slot (as `head`): whether a regiment that
    /// may hold its ground this tick (fighting, standing or queuing) is
    /// listed there, so marching regiments far from any skip the search
    /// for friends in their way. `big_holder`: one too big for the grid is.
    holder: Vec<bool>,
    big_holder: bool,
    /// Marks for visiting each regiment once per search (see `each_near`).
    seen: Vec<Cell<u32>>,
    search: Cell<u32>,
    /// Ground each regiment gives this tick (see [`Combat::push_lines`]).
    give: Vec<FxVec2>,
    volleys: Volleys,
    /// Volleys loosed this tick: (shooter, target), for vision.
    loosed: Vec<(RegimentHandle, RegimentHandle)>,
}

/// Where every regiment is, borrowed field by field from [`Regiments`] so a
/// search can run while the caller changes other fields.
pub(crate) struct Placement<'a> {
    pub alive: &'a [bool],
    pub owner: &'a [PlayerId],
    pub position: &'a [FxVec2],
    pub stances: &'a Stances,
}

/// Borrow where everyone is without borrowing the rest of the pool.
macro_rules! placement {
    ($regs:expr, $stances:expr) => {
        $crate::combat::Placement {
            alive: &$regs.alive,
            owner: &$regs.owner,
            position: &$regs.position,
            stances: $stances,
        }
    };
}
pub(crate) use placement;

/// Whose regiments a search around a point visits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Whose {
    Enemies(PlayerId),
    Friends(PlayerId),
}

struct Volleys {
    alive: Vec<bool>,
    owner: Vec<PlayerId>,
    target: Vec<RegimentHandle>,
    from: Vec<FxVec2>,
    aim: Vec<FxVec2>,
    launch: Vec<u32>,
    impact: Vec<u32>,
    shots: Vec<u16>,
    damage: Vec<u16>,
    accuracy: Vec<u8>,
    free: Vec<u32>,
}

/// One volley in flight, for presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolleyView {
    pub owner: PlayerId,
    pub from: FxVec2,
    pub aim: FxVec2,
    pub launch_tick: u32,
    pub impact_tick: u32,
    pub shots: u16,
}

impl Combat {
    pub fn new(regiment_capacity: usize) -> Combat {
        let none = RegimentHandle {
            index: 0,
            generation: 0,
        };
        Combat {
            head: vec![EMPTY; BUCKETS * OWNER_SLOTS],
            entry_next: vec![EMPTY; regiment_capacity * MAX_CELLS_PER_REGIMENT],
            entry_regiment: vec![0; regiment_capacity * MAX_CELLS_PER_REGIMENT],
            entries: 0,
            big: Vec::with_capacity(regiment_capacity),
            blocks: vec![
                Block {
                    centre: FxVec2::ZERO,
                    forward: FxVec2::ZERO,
                    right: FxVec2::ZERO,
                    half_width: Fx::ZERO,
                    half_depth: Fx::ZERO,
                };
                regiment_capacity
            ],
            radius: vec![Fx::ZERO; regiment_capacity],
            max_radius: Fx::ZERO,
            max_step: GIVE_GROUND,
            holder: vec![false; BUCKETS * OWNER_SLOTS],
            big_holder: false,
            seen: vec![Cell::new(0); regiment_capacity],
            search: Cell::new(0),
            give: vec![FxVec2::ZERO; regiment_capacity],
            volleys: Volleys {
                alive: vec![false; MAX_VOLLEYS],
                owner: vec![0; MAX_VOLLEYS],
                target: vec![none; MAX_VOLLEYS],
                from: vec![FxVec2::ZERO; MAX_VOLLEYS],
                aim: vec![FxVec2::ZERO; MAX_VOLLEYS],
                launch: vec![0; MAX_VOLLEYS],
                impact: vec![0; MAX_VOLLEYS],
                shots: vec![0; MAX_VOLLEYS],
                damage: vec![0; MAX_VOLLEYS],
                accuracy: vec![0; MAX_VOLLEYS],
                free: (0..MAX_VOLLEYS as u32).rev().collect(),
            },
            loosed: Vec::with_capacity(regiment_capacity),
        }
    }

    /// The volleys loosed in the last tick: (shooter, target).
    pub(crate) fn loosed(&self) -> &[(RegimentHandle, RegimentHandle)] {
        &self.loosed
    }

    /// Volleys in flight, in pool order.
    pub fn volleys(&self) -> impl Iterator<Item = VolleyView> + '_ {
        let v = &self.volleys;
        (0..MAX_VOLLEYS)
            .filter(|&i| v.alive[i])
            .map(|i| VolleyView {
                owner: v.owner[i],
                from: v.from[i],
                aim: v.aim[i],
                launch_tick: v.launch[i],
                impact_tick: v.impact[i],
                shots: v.shots[i],
            })
    }

    /// The radius of the biggest block on the field at the start of this
    /// tick (the circle that holds it).
    pub(crate) fn max_radius(&self) -> Fx {
        self.max_radius
    }

    pub(crate) fn max_step(&self) -> Fx {
        self.max_step
    }

    fn bucket_of(cx: i32, cy: i32) -> usize {
        let h = (cx as u32).wrapping_mul(0x9E37_79B1) ^ (cy as u32).wrapping_mul(0x85EB_CA77);
        (h >> 22) as usize & (BUCKETS - 1)
    }

    /// The cells (x0, y0, x1, y1) a circle touches.
    fn cells(p: FxVec2, radius: Fx) -> (i32, i32, i32, i32) {
        let cell = |v: Fx| v.floor_int().div_euclid(CELL_TILES);
        (
            cell(p.x - radius),
            cell(p.y - radius),
            cell(p.x + radius),
            cell(p.y + radius),
        )
    }

    fn build_grid(&mut self, regs: &Regiments, units: &[UnitStats]) {
        self.head.fill(EMPTY);
        self.entries = 0;
        self.big.clear();
        self.max_radius = Fx::ZERO;
        self.max_step = GIVE_GROUND;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            self.blocks[i] = Block::of(regs, units, i);
            self.radius[i] = self.blocks[i].radius();
            self.max_radius = self.max_radius.max(self.radius[i]);
            self.max_step = self.max_step.max(regs.speed[i]);
            let (x0, y0, x1, y1) = Self::cells(regs.position[i], self.radius[i] + REACH);
            if ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize > MAX_CELLS_PER_REGIMENT {
                self.big.push(i as u32);
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let b =
                        Self::bucket_of(x, y) * OWNER_SLOTS + regs.owner[i] as usize % OWNER_SLOTS;
                    let e = self.entries;
                    self.entries += 1;
                    self.entry_regiment[e] = i as u32;
                    self.entry_next[e] = self.head[b];
                    self.head[b] = e as u32;
                }
            }
        }
    }

    /// Call `f` for every live regiment not owned by `friend` whose footprint
    /// may come within `range` of `p` (a superset: callers check the real
    /// distance and whether it is hostile). A regiment can be visited more
    /// than once.
    fn near_enemies(&self, p: FxVec2, range: Fx, friend: PlayerId, mut f: impl FnMut(usize)) {
        let (x0, y0, x1, y1) = Self::cells(p, range);
        let skip = friend as usize % OWNER_SLOTS;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let b = Self::bucket_of(x, y) * OWNER_SLOTS;
                for slot in (0..OWNER_SLOTS).filter(|&s| s != skip) {
                    let mut e = self.head[b + slot];
                    while e != EMPTY {
                        f(self.entry_regiment[e as usize] as usize);
                        e = self.entry_next[e as usize];
                    }
                }
            }
        }
        for &j in &self.big {
            f(j as usize);
        }
    }

    /// Call `f` once for every live regiment of `whose` whose centre is within
    /// `range` of `p`, as the grid saw them at the start of this tick. Friends
    /// are the party's own regiments and its allies'.
    pub(crate) fn each_near(
        &self,
        regs: &Placement,
        p: FxVec2,
        range: Fx,
        whose: Whose,
        f: impl FnMut(usize),
    ) {
        self.each_near_in(regs, p, range, range, whose, f);
    }

    /// [`Combat::each_near`], looking only in the grid cells within `cells`
    /// of `p` (which may be less than `range` when the caller knows every
    /// regiment it wants is filed nearer).
    fn each_near_in(
        &self,
        regs: &Placement,
        p: FxVec2,
        cells: Fx,
        range: Fx,
        whose: Whose,
        mut f: impl FnMut(usize),
    ) {
        let (owner, friends) = match whose {
            Whose::Enemies(owner) => (owner, false),
            Whose::Friends(owner) => (owner, true),
        };
        // Enemies can be in any slot but the party's own; friends only in
        // the slots of the party and its allies.
        let slots = if friends {
            regs.stances.friend_slots(owner, OWNER_SLOTS)
        } else {
            !(1 << (owner as usize % OWNER_SLOTS))
        };
        let range_sq = i64::from(range.raw()) * i64::from(range.raw());
        self.each_filed_once(p, cells, slots, regs.alive, |j| {
            if regs.stances.hostile(owner, regs.owner[j]) != friends
                && ((regs.position[j] - p).length_sq_raw() as i64) <= range_sq
            {
                f(j);
            }
        });
    }

    /// Call `f` for every other live friend of `i` whose bounding circle
    /// touches `i`'s. Any block overlapping `i`'s shares a point inside `i`'s
    /// circle, and every regiment is filed in each cell its circle touches,
    /// so only the cells under `i`'s own circle need looking in.
    fn each_friend_touching(
        &self,
        regs: &Regiments,
        stances: &Stances,
        i: usize,
        mut f: impl FnMut(usize),
    ) {
        let owner = regs.owner[i];
        let slots = stances.friend_slots(owner, OWNER_SLOTS);
        self.each_touching(regs, i, regs.position[i], slots, |j| {
            if !stances.hostile(owner, regs.owner[j]) {
                f(j);
            }
        });
    }

    /// Call `f` for every other live regiment filed under one of the owner
    /// `slots` whose bounding circle touches `i`'s with `i` standing at `p`.
    fn each_touching(
        &self,
        regs: &Regiments,
        i: usize,
        p: FxVec2,
        slots: u32,
        mut f: impl FnMut(usize),
    ) {
        self.each_filed_once(p, self.radius[i], slots, &regs.alive, |j| {
            let r = (self.radius[i] + self.radius[j]).raw() as u64;
            if j != i && (regs.position[j] - p).length_sq_raw() <= r * r {
                f(j);
            }
        });
    }

    /// Call `f` once for every live regiment filed in a grid cell within
    /// `radius` of `p` under one of the owner `slots` (a bit mask), or in
    /// `big`.
    fn each_filed_once(
        &self,
        p: FxVec2,
        radius: Fx,
        slots: u32,
        alive: &[bool],
        mut f: impl FnMut(usize),
    ) {
        let search = self.search.get().wrapping_add(1);
        self.search.set(search);
        if search == 0 {
            // Wrapped: forget every old mark so none can match by accident.
            self.seen.iter().for_each(|c| c.set(u32::MAX));
        }
        let mut visit = |j: usize| {
            if self.seen[j].get() != search && alive[j] {
                self.seen[j].set(search);
                f(j);
            }
        };
        let (x0, y0, x1, y1) = Self::cells(p, radius);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let b = Self::bucket_of(x, y) * OWNER_SLOTS;
                for slot in 0..OWNER_SLOTS {
                    if slots & (1 << slot) == 0 {
                        continue;
                    }
                    let mut e = self.head[b + slot];
                    while e != EMPTY {
                        visit(self.entry_regiment[e as usize] as usize);
                        e = self.entry_next[e as usize];
                    }
                }
            }
        }
        for &j in &self.big {
            visit(j as usize);
        }
    }

    /// Note which grid cells hold regiments that may hold their ground this
    /// tick (see `holder`). Called after [`Combat::before_move`], before
    /// anyone moves.
    pub(crate) fn mark_holders(&mut self, regs: &Regiments) {
        self.holder.fill(false);
        self.big_holder = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i]
                || regs.morale_state[i].broken()
                || !(regs.engaged[i].is_some()
                    || regs.held[i] > 0
                    || (regs.target[i].is_none() && regs.attack_target[i].is_none()))
            {
                continue;
            }
            let (x0, y0, x1, y1) = Self::cells(regs.position[i], self.radius[i]);
            if ((x1 - x0 + 1) * (y1 - y0 + 1)) as usize > MAX_CELLS_PER_REGIMENT {
                self.big_holder = true;
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.holder[Self::bucket_of(x, y) * OWNER_SLOTS
                        + regs.owner[i] as usize % OWNER_SLOTS] = true;
                }
            }
        }
    }

    /// Whether any regiment noted by [`Combat::mark_holders`] in the owner
    /// slots `slots` may be within `range` of `p`.
    fn holder_near(&self, p: FxVec2, range: Fx, slots: u32) -> bool {
        if self.big_holder {
            return true;
        }
        let (x0, y0, x1, y1) = Self::cells(p, range);
        (y0..=y1).any(|y| {
            (x0..=x1).any(|x| {
                let b = Self::bucket_of(x, y) * OWNER_SLOTS;
                (0..OWNER_SLOTS).any(|s| slots & (1 << s) != 0 && self.holder[b + s])
            })
        })
    }

    /// The friendly regiment regiment `i` would push into by moving its
    /// centre to `next`, if any: one whose block it would then overlap by
    /// more than [`CROWDING`], and by no less than it does now (moving out
    /// of an overlap is always allowed). Only friends
    /// holding their ground count: those fighting, waiting in a queue
    /// themselves, or standing still (the last two only for
    /// [`PATIENCE`]). Friends on the march pass through each other, and
    /// routers neither wait nor are waited for.
    pub(crate) fn friend_in_the_way(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        stances: &Stances,
        i: usize,
        next: FxVec2,
    ) -> Result<usize, Fx> {
        if regs.morale_state[i].broken() {
            return Err(Fx::ZERO);
        }
        let patient = regs.held[i] < PATIENCE;
        let me = Block::at(regs, units, i, next);
        let reach = me.radius();
        // How far the search reached before it was narrowed. The holder
        // check and the centre filter keep it, so the result is unchanged:
        // friends that start holding during this tick aren't in the holder
        // grid, and are only found because a marked holder is near them.
        let wide = reach + self.max_radius + LOOK_AHEAD;
        let owner = regs.owner[i];
        if !self.holder_near(next, wide, stances.friend_slots(owner, OWNER_SLOTS)) {
            return Err(LOOK_AHEAD);
        }
        // A friend `j` counts only if its centre is within `reach +
        // radius[j] + 1 + LOOK_AHEAD` of `next` (the circle test below). It
        // started the tick at most `max_step` from where it is now, and is
        // filed in every cell its circle plus REACH touched then, so one of
        // those cells lies within `cells` of `next`.
        let cells = reach + Fx::ONE + LOOK_AHEAD + self.max_step - REACH;
        let mut found: Option<usize> = None;
        let mut gap = LOOK_AHEAD;
        self.each_near_in(
            &placement!(regs, stances),
            next,
            cells,
            wide,
            Whose::Friends(owner),
            |j| {
                if j == i || found.is_some_and(|f| f < j) || regs.morale_state[j].broken() {
                    return;
                }
                let holding = regs.engaged[j].is_some()
                    || (patient
                        && ((1..PATIENCE).contains(&regs.held[j])
                            || (regs.target[j].is_none() && regs.attack_target[j].is_none())));
                if !holding {
                    return;
                }
                // Cheap circle test before the rectangles (radii from the
                // start of the tick, with a tile of slack).
                let touch = reach + self.radius[j] + Fx::ONE;
                let r = (touch + LOOK_AHEAD).raw() as u64;
                let d = (regs.position[j] - next).length_sq_raw();
                if d > r * r {
                    return;
                }
                let r = touch.raw() as u64;
                if d > r * r {
                    gap = gap.min((regs.position[j] - next).length() - touch);
                    return;
                }
                gap = Fx::ZERO;
                let them = Block::of(regs, units, j);
                let after = me.overlap(&them);
                // Where `i` stands now: the same block, before this step.
                let now = Block {
                    centre: regs.position[i],
                    ..me
                };
                if after > CROWDING && after >= now.overlap(&them) {
                    found = Some(j);
                }
            },
        );
        found.ok_or(gap)
    }

    /// Whether regiment `i`, marching along `dir`, could stand beside
    /// friend `j` instead of behind it: the ground is walkable all the way
    /// from where `i` stands to a place level with `j`, on one side or the
    /// other, clear of it.
    pub(crate) fn room_beside(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        nav: &Nav,
        i: usize,
        j: usize,
        dir: FxVec2,
    ) -> bool {
        let right = FxVec2::new(-dir.y, dir.x);
        let me = Block::of(regs, units, i);
        let them = Block::of(regs, units, j);
        let apart = me.extent_along(right) + them.extent_along(right);
        let here = regs.position[i];
        [apart, -apart]
            .iter()
            .any(|&d| nav.clear_line(here, them.centre + right * d))
    }

    /// Contacts, charges and chasing, before anyone moves this tick.
    pub(crate) fn before_move(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        tick: u32,
    ) {
        self.build_grid(regs, units);
        self.crowding(regs, stances, tick);
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            if regs.morale_state[i].broken() {
                // Running for their lives: nobody here is fighting back.
                regs.engaged[i] = None;
                continue;
            }
            let me = self.blocks[i];
            let mut best: Option<(u64, usize)> = None;
            let blocks = &self.blocks;
            let radius = &self.radius;
            self.near_enemies(me.centre, radius[i], regs.owner[i], |j| {
                if !stances.hostile(regs.owner[i], regs.owner[j]) || !regs.alive[j] {
                    return;
                }
                let them = &blocks[j];
                // Cheap circle test before the rectangle one.
                let d = (them.centre - me.centre).length_sq_raw();
                let reach = (radius[i] + radius[j] + REACH).raw() as u64;
                if d > reach * reach || !me.touches(them, REACH) {
                    return;
                }
                if best.is_none_or(|(bd, bj)| (d, j) < (bd, bj)) {
                    best = Some((d, j));
                }
            });
            let previous = regs.engaged[i];
            let was_engaged = previous.is_some();
            // Pushed or turned out of reach mid-fight: press back in, unless
            // given another order.
            if best.is_none()
                && regs.target[i].is_none()
                && regs.attack_target[i].is_none()
                && previous.is_some_and(|h| regs.resolve(h).is_some())
            {
                regs.attack_target[i] = previous;
            }
            let engaged = best.map(|(_, j)| RegimentHandle {
                index: j as u32,
                generation: regs.generation[j],
            });
            if engaged != previous {
                // A new fight, or none: who is winning starts afresh.
                regs.pressing[i] = 0;
                regs.pressed[i] = 0;
                regs.giving[i] = 0;
            }
            regs.engaged[i] = engaged;
            if let Some((_, j)) = best {
                if !was_engaged {
                    // Hitting home at speed is a charge, but nobody charges
                    // out of a river, from behind a shield wall, or while
                    // pushing through friends. A fight ends any march.
                    let charging = regs.target[i].is_some()
                        && regs.pace[i].raw() * 4 >= regs.speed[i].raw() * 3
                        && !terrain.wet(regs.position[i])
                        && regs.formation[i].can_charge()
                        && regs.crowd[i] <= CROWDING;
                    if charging {
                        regs.charge[i] = CHARGE_TICKS;
                    }
                    regs.target[i] = None;
                    // Long weapons hit first when lines meet, if the foe
                    // comes at their points.
                    if units[regs.unit[i] as usize].reach
                        && regs.strike[i] == Strike::Turn
                        && self.blocks[i].side_facing(self.blocks[j].centre) == Side::Front
                    {
                        regs.strike[i] = Strike::Early;
                    }
                }
                regs.target_facing[i] = Some(Angle::of_vec(regs.position[j] - regs.position[i]));
            }
        }

        // Regiments ordered to attack close on their target.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.engaged[i].is_some() || regs.morale_state[i].broken() {
                continue;
            }
            let Some(t) = regs.attack_target[i] else {
                continue;
            };
            let Some(j) = regs.resolve(t) else {
                regs.attack_target[i] = None;
                continue;
            };
            let range = terrain.range(
                units[regs.unit[i] as usize].range,
                regs.position[i],
                regs.position[j],
            );
            let to = regs.position[j] - regs.position[i];
            if range.raw() > 0 && to.length() <= range {
                regs.target[i] = None;
                regs.target_facing[i] = Some(Angle::of_vec(to));
            } else {
                regs.target[i] = Some(regs.position[j]);
                regs.target_facing[i] = None;
            }
        }
    }

    /// How deep each regiment overlaps friendly blocks (`crowd`, measured
    /// every [`CROWD_EVERY`] ticks), and the disorder that follows from it:
    /// full while overlapping by more than [`PUSHING`], fading over
    /// [`DISORDER_TICKS`] once clear. Routers neither crowd nor are
    /// crowded (they are past caring about order).
    fn crowding(&self, regs: &mut Regiments, stances: &Stances, tick: u32) {
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            if regs.morale_state[i].broken() {
                regs.crowd[i] = Fx::ZERO;
                regs.disorder[i] = 0;
                continue;
            }
            if (tick + i as u32).is_multiple_of(CROWD_EVERY) {
                let me = self.blocks[i];
                let mut deepest = Fx::ZERO;
                self.each_friend_touching(regs, stances, i, |j| {
                    // Deeper than this changes nothing (see `SATURATED`).
                    if deepest < SATURATED && !regs.morale_state[j].broken() {
                        deepest = deepest.max(me.overlap(&self.blocks[j]));
                    }
                });
                regs.crowd[i] = deepest.min(SATURATED);
            }
            if regs.crowd[i] > PUSHING {
                regs.disorder(i);
            } else {
                regs.disorder[i] = regs.disorder[i].saturating_sub(1);
            }
        }
    }

    /// Lines that push, after contacts are found and before anyone
    /// marches: each regiment losing its melee exchange (as read at its
    /// last blow, see [`Combat::after_move`]) gives ground straight back
    /// from the side its foe is on, and the enemies fighting it follow, so
    /// the line bends where it is losing.
    /// One with nowhere to go (see [`ground_ends`] and
    /// [`Combat::backs_into`]) stands and is crushed instead: it falls into
    /// disorder and loses heart.
    pub(crate) fn push_lines(
        &mut self,
        regs: &mut Regiments,
        stances: &Stances,
        terrain: &Terrain,
        nav: &Nav,
        map_tiles: u32,
    ) {
        self.give.fill(FxVec2::ZERO);
        let mut anyone = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.giving[i] == 0 {
                continue;
            }
            // Only a foe fighting it back, and not losing a fight of its own,
            // drives it; in a scrum of three or more it holds.
            let Some(j) = regs
                .foe(i)
                .filter(|&j| regs.foe(j) == Some(i) && regs.giving[j] == 0)
            else {
                continue;
            };
            let me = self.blocks[i];
            let foe = self.blocks[j].centre;
            let (axis, across) = if me.side_facing(foe) == Side::Flank {
                (me.right, me.forward)
            } else {
                (me.forward, me.right)
            };
            let away = if (foe - me.centre).dot(axis) >= Fx::ZERO {
                axis * -Fx::ONE
            } else {
                axis
            };
            let pace = Fx::from_raw((GIVE_GROUND.raw() * i32::from(regs.giving[i]) / 100).max(1));
            let give = away * terrain.wading(pace, me.centre);
            if nav.blocks(foe, foe + give) {
                // Its foe can't follow, so nobody is pushing.
                continue;
            }
            // The middle and both corners of its rear edge, after the step.
            let rear = me.centre + away * me.extent_along(away) + give;
            let wing = across * me.extent_along(across);
            if [rear, rear + wing, rear - wing]
                .iter()
                .any(|&p| ground_ends(terrain, nav, map_tiles, me.centre, p))
                || self.backs_into(regs, stances, i, me.centre + give)
            {
                regs.disorder(i);
                regs.shock[i] |= SHOCK_PINNED;
            } else {
                self.give[i] = give;
                anyone = true;
            }
        }
        if !anyone {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let step = if self.give[i] == FxVec2::ZERO {
                self.follow(regs, nav, i)
            } else {
                Some(self.give[i])
            };
            if let Some(step) = step {
                regs.position[i] += step;
            }
        }
    }

    /// The step regiment `i` takes after the foe it fights as that foe
    /// gives ground, if the foe draws away from it. One fighting a foe that
    /// is itself following its own foe follows too, and so on down the
    /// chain (up to [`FOLLOW_HOPS`] fights), so a fight several blocks deep
    /// moves as one; nobody in the chain follows if any of them would step
    /// onto blocked ground. Reads only where everyone stood at the start of
    /// the tick, so the order regiments move in changes nothing.
    fn follow(&self, regs: &Regiments, nav: &Nav, i: usize) -> Option<FxVec2> {
        if regs.giving[i] > 0 {
            // Losing too, and not driven back: it stays where it is.
            return None;
        }
        let mut chain = [i; FOLLOW_HOPS + 1];
        for hop in 1..=FOLLOW_HOPS {
            let k = regs.foe(chain[hop - 1])?;
            if chain[..hop].contains(&k) {
                return None;
            }
            chain[hop] = k;
            let step = self.give[k];
            if step != FxVec2::ZERO {
                // Only if the step draws each foe down the chain away from
                // the one fighting it, and everyone following can take it.
                let draws_away = |w: &[usize]| {
                    step.dot(self.blocks[w[1]].centre - self.blocks[w[0]].centre) > Fx::ZERO
                };
                let can_step = |&k: &usize| {
                    let at = self.blocks[k].centre;
                    !nav.blocks(at, at + step)
                };
                return (chain[..=hop].windows(2).all(draws_away)
                    && chain[..hop].iter().all(can_step))
                .then_some(step);
            }
            if regs.giving[k] > 0 {
                // Losing but not driven back: nobody behind it moves.
                return None;
            }
        }
        None
    }

    /// Whether regiment `i` stepping back to `next` would back into another
    /// block: a friend more than [`PUSHING`] deep or any enemy (unbroken
    /// either way), by more than it overlaps them now.
    fn backs_into(&self, regs: &Regiments, stances: &Stances, i: usize, next: FxVec2) -> bool {
        let now = self.blocks[i];
        let me = Block {
            centre: next,
            ..now
        };
        let owner = regs.owner[i];
        let mut pinned = false;
        self.each_touching(regs, i, next, ALL_SLOTS, |k| {
            if pinned || regs.morale_state[k].broken() {
                return;
            }
            let them = &self.blocks[k];
            let room = if stances.hostile(owner, regs.owner[k]) {
                Fx::ZERO
            } else {
                PUSHING
            };
            let after = me.overlap(them);
            pinned = after > room && after > now.overlap(them);
        });
        pinned
    }

    /// Blows, volleys and deaths, after everyone has moved this tick.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn after_move(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        tick: u32,
        rng: &mut Pcg32,
    ) {
        self.loosed.clear();
        // Melee.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            regs.charge[i] = regs.charge[i].saturating_sub(1);
            regs.reload[i] = regs.reload[i].saturating_sub(1);
            // Each regiment strikes on its turn, once a second. Spears and
            // pikes that have just met their foe strike at once instead, and
            // then sit out their next turn, so lines that touch and part
            // again don't earn them extra blows.
            let on_turn = (tick + i as u32).is_multiple_of(STRIKE_INTERVAL);
            let rhythm = regs.strike[i];
            if on_turn || rhythm == Strike::Early {
                regs.strike[i] = Strike::Turn;
            }
            let Some(j) = regs.foe(i) else {
                continue;
            };
            let strikes = match rhythm {
                Strike::Turn => on_turn,
                Strike::Early => true,
                Strike::Rest => false,
            };
            if !strikes || regs.models[i] == 0 {
                continue;
            }
            if !on_turn {
                regs.strike[i] = Strike::Rest;
            }
            let attacker = &units[regs.unit[i] as usize];
            let defender = &units[regs.unit[j] as usize];
            let me = Block::of(regs, units, i);
            let them = Block::of(regs, units, j);
            let (files, models) = (regs.files[i], regs.models[i]);
            // Soldiers who can reach the foe, in percent of a soldier.
            let fighters = match me.side_facing(them.centre) {
                Side::Front => attacker.front_fighters_percent(files, models),
                Side::Flank => 100 * u64::from(formation::ranks(files, models)),
                Side::Rear => 100 * u64::from(files.min(models).div_ceil(2)),
            };
            let side = them.side_facing(me.centre);
            let defence = defender.melee_defence_in(regs.formation[j], side == Side::Front);
            let hit = hit_percent(i32::from(attacker.melee_attack), defence) as u128;
            let charge = 100
                + u128::from(attacker.charge_bonus) * u128::from(regs.charge[i])
                    / u128::from(CHARGE_TICKS);
            let heart = u128::from(heart_percent(regs.morale_state[i]));
            let room = u128::from(attacker.weapon_room_percent(regs.formation[i]));
            let ground = terrain.melee_percent(regs.position[i], regs.position[j]);
            let (shaken, exposed) = (
                100 - disorder_percent(regs.disorder[i]),
                100 + disorder_percent(regs.disorder[j]),
            );
            // In u128: the extra factors would overflow u64 for big blows.
            // With every new factor at 100 this is exactly the old formula.
            let mut expected = (u128::from(fighters)
                * hit
                * u128::from(attacker.melee_damage)
                * u128::from(side.damage_percent())
                * charge
                * heart
                * room
                * 256
                / (100 * 100 * 100 * 100 * 100 * 100)) as u64
                * ground
                / 100
                * shaken
                / 100
                * exposed
                / 100;
            // Caught off guard by an ambush.
            if vision.off_guard(i) {
                expected = expected * OFF_GUARD_DAMAGE_PERCENT / 100;
            }
            // Even a blow that can't wound shoves.
            let shove = expected
                .max(1)
                .saturating_mul(attacker.mass.heft())
                .min(u64::from(u32::MAX)) as u32;
            regs.pressing[i] = regs.pressing[i].saturating_add(shove);
            regs.pressed[j] = regs.pressed[j].saturating_add(shove);
            deal(regs, units, j, expected, side, rng);
        }

        // Who is winning each melee, read once a second on each regiment's
        // turn, after its blow: then both sides have struck exactly once
        // since the last reading, so the side that struck last doesn't look
        // ahead. Only the last few seconds' blows count.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || !(tick + i as u32).is_multiple_of(STRIKE_INTERVAL) {
                continue;
            }
            regs.pressing[i] -= regs.pressing[i].div_ceil(8);
            regs.pressed[i] -= regs.pressed[i].div_ceil(8);
            regs.giving[i] = driven(regs, i);
        }

        // Ranged regiments standing still loose volleys at the nearest enemy
        // in range (or the one they were told to attack).
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.engaged[i].is_some() || regs.reload[i] > 0 {
                continue;
            }
            let stats = &units[regs.unit[i] as usize];
            if stats.range.raw() <= 0
                || regs.target[i].is_some()
                || regs.models[i] == 0
                || regs.morale_state[i].broken()
            {
                continue;
            }
            let Some(j) = self.pick_target(regs, stances, terrain, vision, i, stats.range) else {
                continue;
            };
            let to = regs.position[j] - regs.position[i];
            let want = Angle::of_vec(to);
            if i32::from(regs.facing[i].delta_to(want)).abs() > 0x2000 {
                regs.target_facing[i] = Some(want);
                continue;
            }
            let Some(v) = self.volleys.free.pop() else {
                continue;
            };
            let v = v as usize;
            let flight = (to.length().raw() / MISSILE_SPEED.raw()).max(1) as u32;
            let target = RegimentHandle {
                index: j as u32,
                generation: regs.generation[j],
            };
            let shooter = RegimentHandle {
                index: i as u32,
                generation: regs.generation[i],
            };
            self.loosed.push((shooter, target));
            let vs = &mut self.volleys;
            vs.alive[v] = true;
            vs.owner[v] = regs.owner[i];
            vs.target[v] = target;
            vs.from[v] = regs.position[i];
            vs.aim[v] = regs.position[j];
            vs.launch[v] = tick;
            vs.impact[v] = tick + flight;
            vs.shots[v] = regs.models[i];
            vs.damage[v] = stats.missile_damage;
            vs.accuracy[v] =
                (u64::from(stats.accuracy) * heart_percent(regs.morale_state[i]) / 100) as u8;
            regs.reload[i] = stats.reload_ticks.max(1);
        }

        // Volleys landing this tick.
        for v in 0..MAX_VOLLEYS {
            let vs = &mut self.volleys;
            if !vs.alive[v] || vs.impact[v] > tick {
                continue;
            }
            vs.alive[v] = false;
            vs.free.push(v as u32);
            let Some(j) = regs.resolve(vs.target[v]) else {
                continue;
            };
            let block = Block::of(regs, units, j);
            // The target walked out from under the arrows.
            if (block.centre - vs.aim[v]).length() > block.radius() + Fx::ONE {
                continue;
            }
            let area = i64::from((block.half_width * block.half_depth).raw()) * 4;
            let density = (i64::from(regs.models[j]) * REFERENCE_AREA_PER_SOLDIER * 100
                / area.max(1))
            .clamp(25, 125) as u64;
            let side = block.side_facing(vs.from[v]);
            // Arrows from the front mostly glance off a shield wall.
            let shields = regs.formation[j].arrow_percent(side == Side::Front);
            let expected = u64::from(vs.shots[v])
                * u64::from(vs.accuracy[v])
                * density
                * u64::from(vs.damage[v])
                * side.damage_percent()
                * 256
                / (100 * 100 * 100)
                * shields
                / 100;
            deal(regs, units, j, expected, side, rng);
        }

        // The fallen.
        for i in 0..regs.alive.len() {
            if regs.alive[i] && regs.models[i] == 0 {
                regs.despawn(RegimentHandle {
                    index: i as u32,
                    generation: regs.generation[i],
                });
            }
        }
    }

    /// The enemy a ranged regiment shoots: the one it was told to attack if
    /// in range, else the nearest in range. Only enemies its side can see;
    /// and while hidden from an enemy, it holds its fire at that enemy
    /// unless ordered to shoot it (docs/20-ambushes.md).
    fn pick_target(
        &self,
        regs: &Regiments,
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        i: usize,
        range: Fx,
    ) -> Option<usize> {
        let me = regs.position[i];
        let owner = regs.owner[i];
        if let Some(j) = regs.attack_target[i].and_then(|t| regs.resolve(t))
            && (regs.position[j] - me).length() <= terrain.range(range, me, regs.position[j])
            && vision.sees(owner, j, regs.owner[j])
        {
            return Some(j);
        }
        let mut best: Option<(u64, usize)> = None;
        // Look as far as high ground could carry, then check each.
        let reach = if terrain.is_flat() {
            range
        } else {
            Terrain::range_from_high_ground(range)
        };
        self.near_enemies(me, reach, owner, |j| {
            if !regs.alive[j]
                || !stances.hostile(owner, regs.owner[j])
                || !vision.sees(owner, j, regs.owner[j])
                // A hidden archer holds its fire at enemies who can't see it.
                || !vision.sees(regs.owner[j], i, owner)
            {
                return;
            }
            let d = (regs.position[j] - me).length_sq_raw();
            let r = i64::from(terrain.range(range, me, regs.position[j]).raw());
            if d <= (r * r) as u64 && best.is_none_or(|(bd, bj)| (d, j) < (bd, bj)) {
                best = Some((d, j));
            }
        });
        best.map(|(_, j)| j)
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        let v = &self.volleys;
        for i in 0..MAX_VOLLEYS {
            if !v.alive[i] {
                continue;
            }
            h.u32(i as u32);
            h.u8(v.owner[i]);
            hash_handle(h, Some(v.target[i]));
            h.i32(v.from[i].x.raw());
            h.i32(v.from[i].y.raw());
            h.i32(v.aim[i].x.raw());
            h.i32(v.aim[i].y.raw());
            h.u32(v.launch[i]);
            h.u32(v.impact[i]);
            h.u16(v.shots[i]);
            h.u16(v.damage[i]);
            h.u8(v.accuracy[i]);
        }
    }
}

/// How much worse, in percent, a regiment this far out of order fights:
/// [`DISORDER_PERCENT`] at full disorder, fading as it re-forms.
fn disorder_percent(disorder: u8) -> u64 {
    DISORDER_PERCENT * u64::from(disorder) / u64::from(DISORDER_TICKS)
}

/// Percent chance a melee blow with `attack` lands on a soldier with
/// `defence` (docs/05-combat.md).
pub fn hit_percent(attack: i32, defence: i32) -> i32 {
    (50 + attack - defence).clamp(10, 90)
}

/// Percent of its usual damage a regiment deals in this state of mind.
pub fn heart_percent(state: MoraleState) -> u64 {
    match state {
        MoraleState::Wavering => WAVERING_DAMAGE_PERCENT,
        _ => 100,
    }
}

/// Apply `expected` damage (in 1/256 hit points) to regiment `j`, rolled
/// within ±25%, landing on its `side`. When a rank empties the block steps
/// forward half a rank, so its front stays where it was (the rear thins; the
/// front rank holds).
fn deal(
    regs: &mut Regiments,
    units: &[UnitStats],
    j: usize,
    expected: u64,
    side: Side,
    rng: &mut Pcg32,
) {
    regs.shock[j] |= match side {
        Side::Front => 0,
        Side::Flank => SHOCK_FLANK,
        Side::Rear => SHOCK_REAR,
    };
    let rolled = expected * (192 + u64::from(rng.below(129))) / 256;
    let total = rolled + u64::from(regs.damage_carry[j]);
    regs.damage_carry[j] = (total % 256) as u16;
    let ranks_before = formation::ranks(regs.files[j], regs.models[j]);
    regs.take_damage(j, (total / 256).min(u64::from(u32::MAX)) as u32);
    let ranks_after = formation::ranks(regs.files[j], regs.models[j]);
    if regs.models[j] > 0 && ranks_after < ranks_before {
        let (_, spacing) = units[regs.unit[j] as usize].spacing(regs.formation[j]);
        let step = Fx::from_raw(spacing.raw() * i32::from(ranks_before - ranks_after) / 2);
        regs.position[j] += regs.facing[j].to_vec() * step;
    }
}
