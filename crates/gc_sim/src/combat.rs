//! Melee and ranged combat (docs/05-combat.md, docs/10-architecture.md).
//!
//! Each tick, in this order:
//! 1. [`Combat::before_move`] finds which enemy blocks touch (oriented
//!    rectangles plus a little reach). A regiment in contact is locked in
//!    melee with the nearest one, and stays locked with it until the lines
//!    are clearly apart; contact made at speed starts a charge. Regiments
//!    ordered to attack chase their target.
//! 2. [`Combat::push_lines`]: a regiment losing its melee gives ground and
//!    the enemies fighting it follow; one with nowhere to go is pinned.
//!    Lines that have drifted a little apart close up again.
//! 3. The regiments move (`Regiments::step_movement`).
//! 4. [`Combat::after_move`] resolves melee blows, looses and lands volleys,
//!    and removes regiments with no soldiers left.
//!
//! Every regiment strikes once a second, staggered by pool index so the work
//! spreads over the ticks. On its turn each of its soldiers who can reach the
//! foe strikes the enemy soldier across from him (see [`duel`]): the blow
//! lands with the chance to hit, and one that lands takes its damage, rolled
//! ±25% on the combat random stream, off that one man. Damage is carried in
//! 1/256ths of a hit point so small hits add up, and a soldier dies when his
//! own hit points run out (docs/28-soldier-combat.md, option B).

use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;

use gc_fixed::{Angle, Fx, FxVec2, Pcg32};

use crate::events::{Event, Shooter, VolleyId};
use crate::morale::{MoraleState, SHOCK_FLANK, SHOCK_PINNED, SHOCK_REAR, WAVERING_DAMAGE_PERCENT};
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments, Strike, hash_handle};
use crate::sky::Air;
use crate::structures::StructureHandle;
use crate::terrain::Terrain;
use crate::vision::{OFF_GUARD_DAMAGE_PERCENT, Vision};
use crate::{PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitStats, formation};

/// Percent of its full weight a melee blow lands with: the one knob for how
/// long fights last. Below 100, locked lines bleed (and so lose heart, see
/// [`crate::morale`]) more slowly and battles draw out without retuning any
/// unit.
pub const BATTLE_PACE_PERCENT: u64 = 70;
/// Percent of its full weight a volley lands with. Arrows are slowed half as
/// much as blows: the march into contact, when most of them are loosed,
/// takes as long as ever, and slowing them as much would rob archers of
/// their share of the fight.
pub const VOLLEY_PACE_PERCENT: u64 = (100 + BATTLE_PACE_PERCENT) / 2;
/// Ticks between a regiment's melee blows.
pub const STRIKE_INTERVAL: u32 = TICKS_PER_SECOND;
/// How far apart two blocks' edges can be and still fight, in tiles.
pub const REACH: Fx = Fx::from_ratio(1, 2);
/// A regiment locked in melee stays locked with its foe until their edges
/// are this far apart, in tiles: lines that shrink as soldiers fall or
/// turn to face each other don't brush apart, and a nearer enemy coming up
/// doesn't pull a regiment off the foe it is fighting.
const LET_GO: Fx = Fx::from_int(2);
/// Locked lines that have drifted more than [`REACH`] apart close up again
/// at this many tiles a tick (a tile a second, slower than a walk).
const CLOSE_UP: Fx = Fx::from_ratio(1, 20);
/// How long a charge bonus lasts after contact, fading to nothing.
pub const CHARGE_TICKS: u16 = 3 * TICKS_PER_SECOND as u16;
/// How many ticks a regiment must have stepped forward since it last halted
/// or fought for hitting home to count as a charge: lines that brush apart
/// and close again, or a block that starts next to its foe, don't charge.
pub const RUN_UP_TICKS: u8 = TICKS_PER_SECOND as u8;
/// Falling back (docs/05-combat.md): a regiment pulls out of its fight to
/// this many tiles straight back from its foe, ...
pub const FALL_BACK_TILES: Fx = Fx::from_int(8);
/// ... and stops wherever it has got to after this long (8 s; at half pace
/// infantry covers the distance in about 5).
pub const FALL_BACK_TICKS: u16 = 8 * TICKS_PER_SECOND as u16;
/// Arrow flight speed, tiles per tick (20 tiles per second).
pub const MISSILE_SPEED: Fx = Fx::ONE;
/// Volleys in flight at once, across all players.
pub const MAX_VOLLEYS: usize = 2048;
/// Room for this many melee blows a tick per regiment the match holds:
/// about one regiment in twenty strikes on any one tick, with at most a
/// regiment's worth of soldiers each, so in all but the biggest battles
/// none are left untold.
const MAX_BLOWS_PER_REGIMENT: usize = 8;
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
/// With nothing blocked within this many tiles there is always room to
/// pass beside a friend, so nobody queues behind one (see
/// [`Combat::friend_in_the_way`]).
pub(crate) const OPEN_GROUND: i32 = 10;
/// How far past touching distance the crowd check looks, so a regiment
/// with no friend that near can march a few ticks without looking again.
const LOOK_AHEAD: Fx = Fx::from_int(2);

/// Lines that push (docs/05-combat.md): a regiment losing the melee
/// exchange gives ground at up to this many tiles a tick (0.4 a second),
/// and the enemies fighting it follow.
pub const GIVE_GROUND: Fx = Fx::from_ratio(1, 50);
/// Charges that plough in (docs/05-combat.md): a charger at the run
/// weighs its mass's heft this many times over...
pub const PLOUGH_SPEED: u64 = 2;
/// ...against the heft of the ranks it hits: up to this many deep on its
/// front, one on its flank or rear (nobody braces a side), weighed by how
/// closely they stand (see [`ploughs`], [`Formation::hold_percent`]).
const PLOUGH_DEPTH: u16 = 4;
/// Ticks a ploughed regiment is driven back, per unit of weight the charge
/// has over it, and at most.
const PLOUGH_TICKS_PER: u64 = 5;
pub const PLOUGH_TICKS: u8 = 40;

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

/// How many ticks regiment `i`, charging home on regiment `j`'s `front`
/// or not, drives `j` back (0: it doesn't plough in): its weight at the run
/// against the weight of the ranks it hits, so cavalry ploughs through thin
/// or loose infantry but stalls on a deep, close block, and anyone hit on
/// the flank or rear is easily bowled over.
fn ploughs(regs: &Regiments, units: &[UnitStats], i: usize, j: usize, front: bool) -> u8 {
    let weight = units[regs.unit[i] as usize].mass.heft() * PLOUGH_SPEED;
    let depth = if front {
        formation::ranks(regs.files[j], regs.models[j]).min(PLOUGH_DEPTH)
    } else {
        1
    };
    let hold = units[regs.unit[j] as usize].mass.heft()
        * u64::from(depth)
        * regs.formation[j].hold_percent(front);
    let over = (weight * 100).saturating_sub(hold) / 100;
    (over * PLOUGH_TICKS_PER).min(u64::from(PLOUGH_TICKS)) as u8
}

/// Spatial hash cell edge, in tiles, and bucket count (a power of two).
const CELL_TILES: i32 = 16;
const BUCKETS: usize = 1024;
const EMPTY: u32 = u32::MAX;
/// The grid keeps each owner's regiments apart (by `owner % OWNER_SLOTS`) so a
/// search for enemies skips crowds of its own regiments, and a search for
/// friends only visits its own and its allies'.
const OWNER_SLOTS: usize = crate::MAX_PARTIES;
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
    party: PlayerId,
    map_tiles: u32,
    centre: FxVec2,
    behind: FxVec2,
) -> bool {
    let edge = Fx::from_int(map_tiles as i32);
    let off_map = map_tiles > 0
        && (behind.x < Fx::ZERO || behind.y < Fx::ZERO || behind.x > edge || behind.y > edge);
    off_map || nav.blocked(party, behind) || (terrain.wet(behind) && !terrain.wet(centre))
}

/// Whether regiments `i` and `j` can't lock in melee because one is falling
/// back from the other, which is in front of it, where it backs away in
/// good order (an enemy at its flank or rear catches it). `j`, the one `i`
/// would lock onto, only gets away after the tick it was ordered to fall
/// back: on that tick its foes still hold it, for their parting blow.
fn parted(regs: &Regiments, blocks: &[Block], i: usize, j: usize) -> bool {
    let backs_away =
        |k: usize, from: usize| blocks[k].side_facing(blocks[from].centre) == Side::Front;
    (regs.falling_back[i] > 0 && backs_away(i, j))
        || ((1..FALL_BACK_TICKS).contains(&regs.falling_back[j]) && backs_away(j, i))
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
    pub const fn damage_percent(self) -> u64 {
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

    /// A block lined up with the map's axes: a building's footprint.
    pub(crate) fn square(centre: FxVec2, half_width: Fx, half_depth: Fx) -> Block {
        Block {
            centre,
            forward: FxVec2::new(Fx::ZERO, Fx::ONE),
            right: FxVec2::new(Fx::ONE, Fx::ZERO),
            half_width,
            half_depth,
        }
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
        self.overlap_over(other, Fx::ZERO).unwrap_or(Fx::ZERO)
    }

    /// [`Block::overlap`], if it is more than `limit` (which is not
    /// negative): stops at the first axis that shows it isn't, as most
    /// pairs tested are close but not that deep.
    fn overlap_over(&self, other: &Block, limit: Fx) -> Option<Fx> {
        debug_assert!(
            limit >= Fx::ZERO,
            "overlap_over needs a limit of zero or more"
        );
        let d = other.centre - self.centre;
        let mut least = Fx::MAX;
        for axis in [self.forward, self.right, other.forward, other.right] {
            let depth = self.extent_along(axis) + other.extent_along(axis) - d.dot(axis).abs();
            if depth <= limit {
                return None;
            }
            least = least.min(depth);
        }
        Some(least)
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
    /// Per regiment: whether it is listed in `holder` this tick.
    marked: Vec<bool>,
    /// Marks for visiting each regiment once per search (see `each_near`).
    seen: Vec<Cell<u32>>,
    search: Cell<u32>,
    /// Ground each regiment gives this tick (see [`Combat::push_lines`]).
    give: Vec<FxVec2>,
    /// Where each regiment steps in [`Combat::push_lines`], worked out for
    /// everyone before anyone moves.
    steps: Vec<Option<FxVec2>>,
    /// Whether any regiment stayed locked with a foe out of reach this
    /// tick, so someone may close up (see [`Combat::close_up`]).
    drifted: bool,
    volleys: Volleys,
    /// Volleys loosed this tick: (volley, shooter, target), for vision
    /// and the tick's events.
    loosed: Vec<(VolleyId, Shooter, RegimentHandle)>,
    /// Tower volleys that came down this tick: the tower, and the hit
    /// points they took (a Tollgate's earnings, see [`crate::towers`]).
    tower_hits: Vec<(StructureHandle, u32)>,
    /// Volleys that came down this tick, for the tick's events (kept only
    /// while `tell`: a rehearsal's world tells no one).
    landed: Vec<Event>,
    /// Melee blows struck this tick, for the screen (kept only while
    /// `tell`, and only as many as there is room for).
    blows: Vec<Blow>,
    tell: bool,
    /// How far the sky lets archers shoot, and how fast troops march (see
    /// [`crate::sky`]).
    air: Air,
}

/// One soldier's melee blow on another (docs/28-soldier-combat.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blow {
    pub striker: RegimentHandle,
    /// The striker's slot in his regiment.
    pub striker_slot: u8,
    pub target: RegimentHandle,
    /// The target's slot in his regiment when the blow landed. A man who
    /// is killed leaves it to the soldier from the regiment's last slot,
    /// who steps up into the gap.
    pub target_slot: u8,
    pub outcome: Outcome,
}

/// What a melee blow did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It missed, was parried or blocked, or glanced off without a wound.
    Parried,
    /// It landed and wounded him.
    Wounded,
    /// It landed and killed him, and with a blow to spare, as many more
    /// (`men` in all) of the soldiers who stepped up into his place.
    Killed { men: u8 },
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
    /// Who loosed it. Which regiment is only told to viewers (it changes
    /// nothing in play, so it isn't hashed); a tower's volley is aimed
    /// shot by shot and earns a Tollgate its toll, so towers are hashed.
    shooter: Vec<Shooter>,
    from: Vec<FxVec2>,
    aim: Vec<FxVec2>,
    launch: Vec<u32>,
    impact: Vec<u32>,
    shots: Vec<u16>,
    damage: Vec<u16>,
    accuracy: Vec<u8>,
    free: Vec<u32>,
}

/// A volley about to be loosed (see [`Combat::loose`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shot {
    pub shooter: Shooter,
    pub owner: PlayerId,
    pub from: FxVec2,
    pub shots: u16,
    /// Hit points each shot that lands takes.
    pub damage: u16,
    /// Percent of shots that land.
    pub accuracy: u8,
}

/// One volley in flight, for presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolleyView {
    pub id: VolleyId,
    /// The regiment or tower that loosed it (it may have fallen since).
    pub shooter: Shooter,
    pub owner: PlayerId,
    pub from: FxVec2,
    pub aim: FxVec2,
    pub launch_tick: u32,
    pub impact_tick: u32,
    pub shots: u16,
}

impl Combat {
    pub(crate) fn set_air(&mut self, air: Air) {
        self.air = air;
    }

    /// What the sky lets troops do now (see [`crate::sky`]).
    pub(crate) fn air(&self) -> Air {
        self.air
    }

    pub fn new(regiment_capacity: usize) -> Combat {
        let none = RegimentHandle {
            index: 0,
            generation: 0,
        };
        Combat {
            air: Air::CLEAR,
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
            marked: vec![false; regiment_capacity],
            seen: vec![Cell::new(0); regiment_capacity],
            search: Cell::new(0),
            give: vec![FxVec2::ZERO; regiment_capacity],
            steps: vec![None; regiment_capacity],
            drifted: false,
            volleys: Volleys {
                alive: vec![false; MAX_VOLLEYS],
                owner: vec![0; MAX_VOLLEYS],
                target: vec![none; MAX_VOLLEYS],
                shooter: vec![Shooter::Regiment(none); MAX_VOLLEYS],
                from: vec![FxVec2::ZERO; MAX_VOLLEYS],
                aim: vec![FxVec2::ZERO; MAX_VOLLEYS],
                launch: vec![0; MAX_VOLLEYS],
                impact: vec![0; MAX_VOLLEYS],
                shots: vec![0; MAX_VOLLEYS],
                damage: vec![0; MAX_VOLLEYS],
                accuracy: vec![0; MAX_VOLLEYS],
                free: (0..MAX_VOLLEYS as u32).rev().collect(),
            },
            loosed: Vec::with_capacity(MAX_VOLLEYS),
            tower_hits: Vec::with_capacity(MAX_VOLLEYS),
            landed: Vec::with_capacity(MAX_VOLLEYS),
            blows: Vec::with_capacity(regiment_capacity * MAX_BLOWS_PER_REGIMENT),
            tell: true,
        }
    }

    /// This combat for a world that tells no one what happens in it.
    pub(crate) fn untold(self) -> Combat {
        Combat {
            landed: Vec::new(),
            blows: Vec::new(),
            tell: false,
            ..self
        }
    }

    /// The melee blows struck in the last tick, in the order they were
    /// struck (none in a world that tells no one). In a very big battle
    /// some may be left out when there is no room for them.
    pub(crate) fn blows(&self) -> &[Blow] {
        &self.blows
    }

    /// The volleys loosed in the last tick: (volley, shooter, target).
    pub(crate) fn loosed(&self) -> &[(VolleyId, Shooter, RegimentHandle)] {
        &self.loosed
    }

    /// The tower volleys that came down in the last tick: the tower, and
    /// the hit points they took.
    pub(crate) fn tower_hits(&self) -> &[(StructureHandle, u32)] {
        &self.tower_hits
    }

    /// Loose `shot` at regiment slot `j`: it flies at [`MISSILE_SPEED`] to
    /// where the target stands now. False when every volley is in flight.
    pub(crate) fn loose(&mut self, regs: &Regiments, j: usize, shot: Shot, tick: u32) -> bool {
        let Some(v) = self.volleys.free.pop() else {
            return false;
        };
        let v = v as usize;
        let aim = regs.position[j];
        let flight = ((aim - shot.from).length().raw() / MISSILE_SPEED.raw()).max(1) as u32;
        let target = regs.handle(j);
        self.loosed.push((
            VolleyId {
                slot: v as u32,
                launch_tick: tick,
            },
            shot.shooter,
            target,
        ));
        let vs = &mut self.volleys;
        vs.alive[v] = true;
        vs.shooter[v] = shot.shooter;
        vs.owner[v] = shot.owner;
        vs.target[v] = target;
        vs.from[v] = shot.from;
        vs.aim[v] = aim;
        vs.launch[v] = tick;
        vs.impact[v] = tick + flight;
        vs.shots[v] = shot.shots;
        vs.damage[v] = shot.damage;
        vs.accuracy[v] = shot.accuracy;
        true
    }

    /// The volleys that came down in the last tick, as events.
    pub(crate) fn landed(&self) -> &[Event] {
        &self.landed
    }

    /// Volleys in flight, in pool order.
    pub fn volleys(&self) -> impl Iterator<Item = VolleyView> + '_ {
        let v = &self.volleys;
        (0..MAX_VOLLEYS)
            .filter(|&i| v.alive[i])
            .map(|i| VolleyView {
                id: VolleyId {
                    slot: i as u32,
                    launch_tick: v.launch[i],
                },
                shooter: v.shooter[i],
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
            self.max_step = self.max_step.max(regs.marched(i, regs.speed[i]));
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
    pub(crate) fn near_enemies(
        &self,
        p: FxVec2,
        range: Fx,
        friend: PlayerId,
        mut f: impl FnMut(usize),
    ) {
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
        mut f: impl FnMut(usize),
    ) {
        self.each_near_in(regs, p, range, range, whose, |j, _| f(j));
    }

    /// [`Combat::each_near`], looking only in the grid cells within `cells`
    /// of `p` (which may be less than `range` when the caller knows every
    /// regiment it wants is filed nearer), and telling `f` each one's
    /// squared distance from `p` (raw).
    fn each_near_in(
        &self,
        regs: &Placement,
        p: FxVec2,
        cells: Fx,
        range: Fx,
        whose: Whose,
        mut f: impl FnMut(usize, u64),
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
            if regs.stances.hostile(owner, regs.owner[j]) == friends {
                return;
            }
            let d = (regs.position[j] - p).length_sq_raw();
            if d as i64 <= range_sq {
                f(j, d);
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
        // Up on a wall, a regiment touches and is touched by nobody.
        if regs.on_wall[i] {
            return;
        }
        self.each_filed_once(p, self.radius[i], slots, &regs.alive, |j| {
            let r = (self.radius[i] + self.radius[j]).raw() as u64;
            if j != i && !regs.on_wall[j] && (regs.position[j] - p).length_sq_raw() <= r * r {
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
        // Most entries are of regiments already seen in another cell, so
        // this test stays inline in the loops and only new ones call `f`.
        let first = |j: usize| {
            let new = self.seen[j].get() != search && alive[j];
            if new {
                self.seen[j].set(search);
            }
            new
        };
        let (x0, y0, x1, y1) = Self::cells(p, radius);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let b = Self::bucket_of(x, y) * OWNER_SLOTS;
                let mut left = slots & ((1 << OWNER_SLOTS) - 1);
                while left != 0 {
                    let slot = left.trailing_zeros() as usize;
                    left &= left - 1;
                    let mut e = self.head[b + slot];
                    while e != EMPTY {
                        let j = self.entry_regiment[e as usize] as usize;
                        if first(j) {
                            f(j);
                        }
                        e = self.entry_next[e as usize];
                    }
                }
            }
        }
        for &j in &self.big {
            let j = j as usize;
            if first(j) {
                f(j);
            }
        }
    }

    /// Note which grid cells hold regiments that may hold their ground this
    /// tick (see `holder`). Called after [`Combat::before_move`], before
    /// anyone moves.
    pub(crate) fn mark_holders(&mut self, regs: &Regiments, nav: &Nav) {
        self.holder.fill(false);
        self.big_holder = false;
        self.marked.fill(false);
        for i in 0..regs.alive.len() {
            if !regs.alive[i]
                || regs.morale_state[i].broken()
                || regs.on_wall[i]
                || !(regs.engaged[i].is_some()
                    || regs.held[i] > 0
                    || (regs.target[i].is_none() && regs.attack_target[i].is_none())
                    || !nav.open_around(regs.owner[i], regs.position[i], OPEN_GROUND))
            {
                continue;
            }
            self.marked[i] = true;
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
    /// [`PATIENCE`]); and, also for [`PATIENCE`], friends marching ahead
    /// of it the same way (facing within a right angle of `dir`, its way),
    /// so blocks file through a ford one behind another instead of piling
    /// into each other. Routers neither wait nor are waited for. Only
    /// called in tight ground (see [`OPEN_GROUND`]); in the open, friends
    /// on the march pass through each other.
    pub(crate) fn friend_in_the_way(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        stances: &Stances,
        i: usize,
        next: FxVec2,
        dir: FxVec2,
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
            |j, d| {
                if j == i
                    || found.is_some_and(|f| f < j)
                    || regs.morale_state[j].broken()
                    || regs.on_wall[j]
                {
                    return;
                }
                // Cheap circle test before the rectangles (radii from the
                // start of the tick, with a tile of slack).
                let touch = reach + self.radius[j] + Fx::ONE;
                let r = (touch + LOOK_AHEAD).raw() as u64;
                if d > r * r {
                    return;
                }
                // A friend marching on ahead only counts in tight ground
                // too (as `mark_holders` lists it): in the open, friends
                // on the march pass through each other.
                let ahead = || {
                    self.marked[j]
                        && regs.target[j].is_some()
                        && (regs.position[j] - regs.position[i]).dot(dir) > Fx::ZERO
                        && regs.facing[j].to_vec().dot(dir) > Fx::ZERO
                };
                let holding = regs.engaged[j].is_some()
                    || (patient
                        && ((1..PATIENCE).contains(&regs.held[j])
                            || (regs.target[j].is_none() && regs.attack_target[j].is_none())
                            || ahead()));
                if !holding {
                    return;
                }
                let r = touch.raw() as u64;
                if d > r * r {
                    gap = gap.min((regs.position[j] - next).length() - touch);
                    return;
                }
                gap = Fx::ZERO;
                let them = Block::of(regs, units, j);
                // Where `i` stands now: the same block, before this step.
                let now = Block {
                    centre: regs.position[i],
                    ..me
                };
                if me
                    .overlap_over(&them, CROWDING)
                    .is_some_and(|after| now.overlap_over(&them, after).is_none())
                {
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
            .any(|&d| nav.clear_line(regs.owner[i], here, them.centre + right * d))
    }

    /// Contacts, charges and chasing, before anyone moves this tick.
    pub(crate) fn before_move(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        tick: u32,
    ) {
        self.build_grid(regs, units);
        self.crowding(regs, stances, tick);
        self.drifted = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            if regs.morale_state[i].broken() {
                // Running for their lives: nobody here is fighting back, or
                // backing away in order, or standing braced.
                regs.engaged[i] = None;
                regs.falling_back[i] = 0;
                regs.braced[i] = false;
                continue;
            }
            let me = self.blocks[i];
            // Locked in: a regiment keeps fighting the foe it is touching,
            // unless one of them is backing away from the other.
            let foe = regs.foe(i).filter(|&j| {
                stances.hostile(regs.owner[i], regs.owner[j])
                    && !regs.morale_state[j].broken()
                    && !parted(regs, &self.blocks, i, j)
                    && regs.meet(i, j)
            });
            let mut best = foe.filter(|&j| me.touches(&self.blocks[j], REACH));
            if best.is_none() {
                let mut nearest: Option<(u64, usize)> = None;
                let blocks = &self.blocks;
                let radius = &self.radius;
                self.near_enemies(me.centre, radius[i], regs.owner[i], |j| {
                    // Melee can't reach up a wall, nor down from one.
                    if !stances.hostile(regs.owner[i], regs.owner[j])
                        || !regs.alive[j]
                        || !regs.meet(i, j)
                    {
                        return;
                    }
                    let them = &blocks[j];
                    // Cheap circle test before the rectangle one.
                    let d = (them.centre - me.centre).length_sq_raw();
                    let reach = (radius[i] + radius[j] + REACH).raw() as u64;
                    if d > reach * reach || !me.touches(them, REACH) || parted(regs, blocks, i, j) {
                        return;
                    }
                    if nearest.is_none_or(|(bd, bj)| (d, j) < (bd, bj)) {
                        nearest = Some((d, j));
                    }
                });
                best = nearest.map(|(_, j)| j);
            }
            if best.is_none() {
                // Drifted a little apart from its foe, with no other enemy in
                // reach: it stays locked while it closes up again (striking
                // only once back in reach, see `after_move`).
                best = foe.filter(|&j| me.touches(&self.blocks[j], LET_GO));
                self.drifted |= best.is_some();
            }
            let previous = regs.engaged[i];
            let was_engaged = previous.is_some();
            // Pushed or turned out of reach mid-fight: press back in, unless
            // given another order, standing braced, or the foe is falling
            // back (it is let go).
            if best.is_none()
                && regs.target[i].is_none()
                && regs.attack_target[i].is_none()
                && !regs.braced[i]
                && previous
                    .and_then(|h| regs.resolve(h))
                    .is_some_and(|j| regs.falling_back[j] == 0)
            {
                regs.attack_target[i] = previous;
            }
            let engaged = best.map(|j| RegimentHandle {
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
            if let Some(j) = best {
                if !was_engaged {
                    // Hitting home at speed after a run-up is a charge, but
                    // nobody charges out of a river or mud, or into either,
                    // from behind a shield wall, or while pushing through
                    // friends. A fight ends any march.
                    let charging = regs.target[i].is_some()
                        && regs.run_up[i] >= RUN_UP_TICKS
                        && regs.pace[i].raw() * 4 >= regs.speed[i].raw() * 3
                        && terrain.bears_charge(regs.position[i])
                        && terrain.bears_charge(regs.position[j])
                        && regs.formation[i].can_charge()
                        && regs.crowd[i] <= CROWDING;
                    let front = || self.blocks[j].side_facing(me.centre) == Side::Front;
                    if charging && regs.braced[j] && front() {
                        // Onto braced points: the charge is spent on them,
                        // its weight turned back on the charger, and the
                        // charger, stopped dead, loses its next blow and
                        // its ranks are thrown into disorder. (The spears
                        // strike at once, as long weapons do when a foe
                        // comes at their points.)
                        let (mine, theirs) = (
                            terrain.charge_bonus(
                                units[regs.unit[i] as usize].charge_bonus,
                                regs.position[i],
                                regs.position[j],
                            ),
                            units[regs.unit[j] as usize].charge_bonus,
                        );
                        regs.charge[j] = CHARGE_TICKS;
                        regs.charge_bonus[j] = mine.max(theirs);
                        regs.disorder(i);
                        regs.strike[i] = Strike::Rest;
                    } else if charging {
                        regs.charge[i] = CHARGE_TICKS;
                        regs.charge_bonus[i] = terrain.charge_bonus(
                            units[regs.unit[i] as usize].charge_bonus,
                            regs.position[i],
                            regs.position[j],
                        );
                        // Heavy enough, it ploughs into the ranks: they are
                        // driven back from it, and the charger strikes the
                        // men it knocks aside at once. (Routers are only
                        // run down.)
                        // Up onto high ground, it hits home too slowly to
                        // plough in.
                        let uphill = terrain.downhill(regs.position[j], regs.position[i]);
                        let ploughed = if regs.morale_state[j].broken() || uphill {
                            0
                        } else {
                            ploughs(regs, units, i, j, front())
                        };
                        if ploughed > 0 {
                            regs.ploughed[j] = ploughed;
                            regs.ploughed_by[j] = Some(regs.handle(i));
                            // Bowled over from a flank or rear, it is hit
                            // hard enough already (see `Side::damage_percent`).
                            if front() {
                                regs.disorder(j);
                            }
                            if regs.strike[i] == Strike::Turn {
                                regs.strike[i] = Strike::Early;
                            }
                        }
                    }
                    // Caught from the flank or rear while falling back.
                    regs.falling_back[i] = 0;
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
                vision.reach_at(units[regs.unit[i] as usize].range, j),
                regs.position[i],
                regs.position[j],
            );
            let to = regs.position[j] - regs.position[i];
            // At the front of a block falling back, it can't lock it, so it
            // waits there rather than walking into the block.
            let waits =
                parted(regs, &self.blocks, i, j) && self.blocks[i].touches(&self.blocks[j], REACH);
            if regs.on_wall[i] {
                // The wall holds it: it shoots what comes in range.
                continue;
            }
            if regs.on_wall[j] && range.raw() == 0 {
                // Melee from the ground can't reach a regiment on a wall.
                regs.attack_target[i] = None;
                continue;
            }
            // A target only heard in the dark is closed with until it is
            // seen, however far the bows reach.
            let shootable = range.raw() > 0
                && to.length() <= range
                && vision.sees(regs.owner[i], j, regs.owner[j]);
            if waits || shootable {
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
                    if deepest < SATURATED
                        && !regs.morale_state[j].broken()
                        && let Some(more) = me.overlap_over(&self.blocks[j], deepest)
                    {
                        deepest = more;
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
    /// disorder and loses heart. Locked lines that have drifted apart close
    /// up (see [`Combat::close_up`]).
    pub(crate) fn push_lines(
        &mut self,
        regs: &mut Regiments,
        stances: &Stances,
        terrain: &Terrain,
        units: &[UnitStats],
        nav: &Nav,
        map_tiles: u32,
    ) {
        self.give.fill(FxVec2::ZERO);
        let mut anyone = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let giving = regs.driven_pace(i);
            if giving == 0 {
                continue;
            }
            // Only a foe fighting it back, and not losing a fight of its own,
            // drives it (the charge that ploughed into it, while that lasts);
            // in a scrum of three or more it holds.
            let Some(j) = regs
                .plougher(i)
                .or_else(|| regs.foe(i))
                .filter(|&j| regs.foe(j) == Some(i) && regs.driven_pace(j) == 0)
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
            let pace = Fx::from_raw((GIVE_GROUND.raw() * i32::from(giving) / 100).max(1));
            let mass = units[regs.unit[i] as usize].mass;
            let give = away * self.air.march(terrain.going(pace, me.centre, away, mass));
            if nav.blocks(regs.owner[j], foe, foe + give) {
                // Its foe can't follow, so nobody is pushing.
                continue;
            }
            // The middle and both corners of its rear edge, after the step.
            let rear = me.centre + away * me.extent_along(away) + give;
            let wing = across * me.extent_along(across);
            if [rear, rear + wing, rear - wing]
                .iter()
                .any(|&p| ground_ends(terrain, nav, regs.owner[i], map_tiles, me.centre, p))
                || self.backs_into(regs, stances, i, me.centre + give)
            {
                regs.disorder(i);
                regs.shock[i] |= SHOCK_PINNED;
            } else {
                self.give[i] = give;
                anyone = true;
            }
        }
        if !anyone && !self.drifted {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                self.steps[i] = None;
                continue;
            }
            self.steps[i] = if self.give[i] != FxVec2::ZERO {
                Some(self.give[i])
            } else if anyone {
                self.follow(regs, nav, i)
            } else {
                None
            }
            .or_else(|| {
                self.drifted
                    .then(|| self.close_up(regs, stances, terrain, units, nav, i))
                    .flatten()
            });
        }
        for (p, step) in regs.position.iter_mut().zip(&self.steps) {
            if let Some(step) = step {
                *p += *step;
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
        if regs.driven_pace(i) > 0 {
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
            // A regiment a charge ploughed into is followed only by its
            // charger: the foe it was fighting doesn't slide along with it.
            if step != FxVec2::ZERO && regs.plougher(k).is_some_and(|p| p != chain[hop - 1]) {
                return None;
            }
            if step != FxVec2::ZERO {
                // Only if the step draws each foe down the chain away from
                // the one fighting it, and everyone following can take it.
                let draws_away = |w: &[usize]| {
                    step.dot(self.blocks[w[1]].centre - self.blocks[w[0]].centre) > Fx::ZERO
                };
                let can_step = |&k: &usize| {
                    let at = self.blocks[k].centre;
                    !nav.blocks(regs.owner[k], at, at + step)
                };
                return (chain[..=hop].windows(2).all(draws_away)
                    && chain[..hop].iter().all(can_step))
                .then_some(step);
            }
            if regs.driven_pace(k) > 0 {
                // Losing but not driven back: nobody behind it moves.
                return None;
            }
        }
        None
    }

    /// The step regiment `i` takes toward the foe it is locked with when
    /// their lines have drifted more than [`REACH`] apart (but less than
    /// [`LET_GO`]), unless either is giving ground (lines that push move
    /// them, see [`Combat::follow`]), the ground is blocked, or it would push
    /// into another block. Reads only where everyone stood at the start of
    /// the tick.
    fn close_up(
        &self,
        regs: &Regiments,
        stances: &Stances,
        terrain: &Terrain,
        units: &[UnitStats],
        nav: &Nav,
        i: usize,
    ) -> Option<FxVec2> {
        let j = regs.foe(i)?;
        let (me, them) = (self.blocks[i], self.blocks[j]);
        if regs.driven_pace(i) > 0
            || regs.driven_pace(j) > 0
            || regs.morale_state[i].broken()
            || me.touches(&them, REACH)
        {
            return None;
        }
        let mass = units[regs.unit[i] as usize].mass;
        let toward = (them.centre - me.centre).normalized();
        let step = toward
            * self
                .air
                .march(terrain.going(CLOSE_UP, me.centre, toward, mass));
        let next = me.centre + step;
        (!nav.blocks(regs.owner[i], me.centre, next) && !self.backs_into(regs, stances, i, next))
            .then_some(step)
    }

    /// Whether regiment `i` stepping to `next` would back (or close) into another
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
        nav: &Nav,
        tick: u32,
        rng: &mut Pcg32,
    ) {
        self.loosed.clear();
        self.landed.clear();
        self.blows.clear();
        self.tower_hits.clear();
        // Melee.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            regs.charge[i] = regs.charge[i].saturating_sub(1);
            regs.ploughed[i] = regs.ploughed[i].saturating_sub(1);
            // A hasted regiment reloads faster and has extra turns (see
            // [`crate::powers`]).
            regs.reload[i] = regs.reload[i].saturating_sub(regs.steps(i));
            // Each regiment strikes on its turn, once a second. Spears and
            // pikes that have just met their foe strike at once instead, and
            // then sit out their next turn, so lines that touch and part
            // again don't earn them extra blows.
            let on_turn =
                (tick + i as u32).is_multiple_of(STRIKE_INTERVAL) || regs.extra_turn(i, tick);
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
            // A parting blow lands on a foe pulling out of a lock however
            // far apart the locked lines had drifted.
            let reach = if rhythm == Strike::Early && regs.falling_back[j] > 0 {
                LET_GO
            } else {
                REACH
            };
            if !me.touches(&them, reach) {
                // Locked but drifted apart: closing up, out of reach.
                continue;
            }
            let (files, models) = (regs.files[i], regs.models[i]);
            // Soldiers who can reach the foe, in percent of a soldier.
            // At the head of a ladder the fight is head on, and only a few
            // wide (see `crate::manning`).
            let ladder = regs.climber[i] || regs.climber[j];
            let (my_side, side, width) = if ladder {
                (Side::Front, Side::Front, crate::manning::LADDER_FRONT)
            } else {
                (
                    me.side_facing(them.centre),
                    them.side_facing(me.centre),
                    u16::MAX,
                )
            };
            let fighters = match my_side {
                Side::Front => attacker.front_fighters_percent(files, models),
                Side::Flank => 100 * u64::from(formation::ranks(files, models)),
                Side::Rear => 100 * u64::from(files.min(models).div_ceil(2)),
            }
            .min(100 * u64::from(width));
            // Not marching: a disciplined regiment holds its ground (see
            // `UnitStats::melee_defence_in`).
            let defence =
                defender.melee_defence_in(regs.formation[j], side == Side::Front, regs.still(j));
            let hit = hit_percent(i32::from(attacker.melee_attack), defence) as u32;
            let charge = 100
                + u128::from(regs.charge_bonus[i]) * u128::from(regs.charge[i])
                    / u128::from(CHARGE_TICKS);
            let heart = u128::from(heart_percent(regs.morale_state[i]));
            let room = u128::from(attacker.weapon_room_percent(regs.formation[i]));
            let ground = terrain.melee_percent(regs.position[i], regs.position[j]);
            let (shaken, exposed) = (
                100 - disorder_percent(regs.disorder[i]),
                100 + disorder_percent(regs.disorder[j]),
            );
            // What one soldier's blow that lands does, in 1/256 hit points.
            // In u128: the extra factors would overflow u64 for big blows.
            let mut landed = (u128::from(attacker.melee_damage)
                * u128::from(side.damage_percent())
                * charge
                * heart
                * room
                * 256
                / (100 * 100 * 100 * 100)) as u64
                * ground
                / 100
                * shaken
                / 100
                * exposed
                / 100;
            // Caught off guard by an ambush.
            if vision.off_guard(i) {
                landed = landed * OFF_GUARD_DAMAGE_PERCENT / 100;
            }
            // Undead driven on by their general (see `generals`; `led` is
            // from the tick before, as generals update after combat).
            if regs.led[i] && attacker.morale == 0 {
                landed = landed * crate::generals::DRIVEN_PERCENT / 100;
            }
            if regs.climber[i] {
                landed = landed * crate::manning::LADDER_DAMAGE_PERCENT / 100;
            } else if regs.climber[j] && attacker.reach {
                landed = landed * crate::manning::REACH_VS_LADDER_PERCENT / 100;
            }
            landed = landed * BATTLE_PACE_PERCENT / 100;
            // Even a blow that can't wound shoves, by the weight of all the
            // blows the regiment strikes.
            let expected = landed * fighters * u64::from(hit) / (100 * 100);
            let shove = expected
                .max(1)
                .saturating_mul(attacker.mass.heft())
                .min(u64::from(u32::MAX)) as u32;
            regs.pressing[i] = regs.pressing[i].saturating_add(shove);
            regs.pressed[j] = regs.pressed[j].saturating_add(shove);
            let blows = (self.tell).then_some(&mut self.blows);
            duel(
                regs,
                units,
                nav,
                Fight {
                    i,
                    j,
                    my_side,
                    side,
                    hit,
                    landed,
                    // No second rank reaches past a ladder head.
                    reach: attacker.reach && !ladder,
                    width,
                },
                rng,
                blows,
            );
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
            // Up on a wall it shoots further, and along the wall's whole
            // length without turning (not from the head of a ladder).
            let mut range = stats.range;
            if regs.on_wall[i] && !regs.climber[i] {
                range = range * crate::manning::WALL_RANGE_PERCENT / 100;
            }
            let Some(j) = self.pick_target(regs, stances, terrain, vision, i, range) else {
                continue;
            };
            let to = regs.position[j] - regs.position[i];
            let want = Angle::of_vec(to);
            if !regs.on_wall[i] && i32::from(regs.facing[i].delta_to(want)).abs() > 0x2000 {
                regs.target_facing[i] = Some(want);
                continue;
            }
            let shot = Shot {
                shooter: Shooter::Regiment(regs.handle(i)),
                owner: regs.owner[i],
                from: regs.position[i],
                shots: regs.models[i],
                damage: stats.missile_damage,
                accuracy: (u64::from(stats.accuracy) * heart_percent(regs.morale_state[i]) / 100)
                    as u8,
            };
            if !self.loose(regs, j, shot, tick) {
                continue;
            }
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
            let landed = |lost| Event::Landed {
                volley: VolleyId {
                    slot: v as u32,
                    launch_tick: vs.launch[v],
                },
                target: vs.target[v],
                at: vs.aim[v],
                lost,
            };
            let Some(j) = regs.resolve(vs.target[v]) else {
                if self.tell {
                    self.landed.push(landed(0));
                }
                continue;
            };
            let block = Block::of(regs, units, j);
            // The target walked out from under the arrows.
            if (block.centre - vs.aim[v]).length() > block.radius() + Fx::ONE {
                if self.tell {
                    self.landed.push(landed(0));
                }
                continue;
            }
            // A tower's shots are aimed one by one, so how close the target
            // stands makes no odds; a regiment's volley falls on the block.
            let tower = vs.shooter[v].tower();
            let density = if tower.is_some() {
                100
            } else {
                let area = i64::from((block.half_width * block.half_depth).raw()) * 4;
                (i64::from(regs.models[j]) * REFERENCE_AREA_PER_SOLDIER * 100 / area.max(1))
                    .clamp(25, 125) as u64
            };
            let side = block.side_facing(vs.from[v]);
            // Arrows from the front mostly glance off a shield wall, and
            // trees catch some of those loosed into or out of a wood.
            let shields = regs.formation[j].arrow_percent(side == Side::Front);
            // Judged where the arrows were aimed, as the arrows on screen are.
            let trees = terrain.through_trees(vs.from[v], vs.aim[v]);
            let expected = u64::from(vs.shots[v])
                * u64::from(vs.accuracy[v])
                * density
                * u64::from(vs.damage[v])
                * side.damage_percent()
                * 256
                / (100 * 100 * 100)
                * shields
                * trees
                / (100 * 100)
                * VOLLEY_PACE_PERCENT
                / 100;
            // A wall's parapet covers those on it, not those still on
            // their ladders.
            let expected = if regs.on_wall[j] && !regs.climber[j] {
                expected * crate::manning::WALL_COVER_PERCENT / 100
            } else {
                expected
            };
            let (lost, hp) = deal(regs, units, nav, j, expected, side, rng);
            if let Some(t) = tower {
                self.tower_hits.push((t, hp));
            }
            if self.tell {
                self.landed.push(landed(lost));
            }
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
        // Further at a target in torchlight (see [`Vision::reach_at`]).
        let range_at = |j: usize| vision.reach_at(range, j);
        if let Some(j) = regs.attack_target[i].and_then(|t| regs.resolve(t))
            && (regs.position[j] - me).length() <= terrain.range(range_at(j), me, regs.position[j])
            && vision.sees(owner, j, regs.owner[j])
        {
            return Some(j);
        }
        let mut best: Option<(u64, usize)> = None;
        // Look as far as high ground could carry, then check each.
        let reach = terrain.reach(vision.reach_any(range));
        self.near_enemies(me, reach, owner, |j| {
            if !regs.alive[j]
                || !stances.hostile(owner, regs.owner[j])
                || !vision.sees(owner, j, regs.owner[j])
                // A hidden archer holds its fire at enemies who can't see it
                // for cover (being out of their sight in fog of war is not
                // hiding).
                || vision.holds_fire(regs.owner[j], i, owner)
            {
                return;
            }
            let d = (regs.position[j] - me).length_sq_raw();
            if terrain.in_range(range_at(j), me, regs.position[j])
                && best.is_none_or(|(bd, bj)| (d, j) < (bd, bj))
            {
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
            if let Shooter::Tower(t) = v.shooter[i] {
                h.u32(t.index);
                h.u16(t.generation);
            }
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
/// within ±25%, landing on its `side`; the soldiers lost, and the hit
/// points taken. When a rank empties the block steps forward half a rank,
/// so its front stays where it was (the rear thins; the front rank holds),
/// unless that would step it onto blocked ground.
fn deal(
    regs: &mut Regiments,
    units: &[UnitStats],
    nav: &Nav,
    j: usize,
    expected: u64,
    side: Side,
    rng: &mut Pcg32,
) -> (u16, u32) {
    struck_on(regs, j, side);
    let whole = carry(regs, j, roll(expected, rng));
    let ranks_before = formation::ranks(regs.files[j], regs.models[j]);
    let taken = regs.take_damage_hp(j, whole.min(u64::from(u32::MAX)) as u32);
    step_up(regs, units, nav, j, ranks_before);
    taken
}

/// Marks regiment `j` as struck on `side`, for its next morale check.
fn struck_on(regs: &mut Regiments, j: usize, side: Side) {
    regs.shock[j] |= match side {
        Side::Front => 0,
        Side::Flank => SHOCK_FLANK,
        Side::Rear => SHOCK_REAR,
    };
}

/// `expected` damage rolled ±25% on the combat random stream.
fn roll(expected: u64, rng: &mut Pcg32) -> u64 {
    expected * (192 + u64::from(rng.below(129))) / 256
}

/// Adds `rolled` (1/256 hit points) to regiment `j`'s carried fraction and
/// returns the whole hit points it makes, keeping the rest.
fn carry(regs: &mut Regiments, j: usize, rolled: u64) -> u64 {
    let total = rolled + u64::from(regs.damage_carry[j]);
    regs.damage_carry[j] = (total % 256) as u16;
    total / 256
}

/// One regiment's melee turn against its foe, as [`duel`] plays it.
struct Fight {
    /// The striking regiment and its foe.
    i: usize,
    j: usize,
    /// The striker's side that faces the foe, and the foe's side that faces
    /// the striker.
    my_side: Side,
    side: Side,
    /// The chance each blow lands, in percent.
    hit: u32,
    /// What one blow that lands does, in 1/256 hit points, before its roll.
    landed: u64,
    /// Spears and pikes: the second rank fights too.
    reach: bool,
    /// The most soldiers abreast who strike (a ladder head's few).
    width: u16,
}

/// The soldiers of a regiment along one side of its block, in order from
/// one end to the other.
#[derive(Clone, Copy)]
struct Edge {
    side: Side,
    /// For a flank, which: the end of each rank with the last file, or the
    /// first.
    last_file: bool,
    files: u16,
    models: u16,
}

impl Edge {
    fn of(regs: &Regiments, i: usize, side: Side, last_file: bool) -> Edge {
        let models = regs.models[i];
        Edge {
            side,
            last_file,
            files: regs.files[i].clamp(1, models.max(1)),
            models,
        }
    }

    fn ranks(&self) -> u16 {
        formation::ranks(self.files, self.models)
    }

    /// How many soldiers stand along it.
    fn len(&self) -> u16 {
        match self.side {
            Side::Front => self.files.min(self.models),
            Side::Rear => self.models - (self.ranks() - 1) * self.files,
            Side::Flank => self.ranks(),
        }
    }

    /// The slot of its `n`th soldier.
    fn slot(&self, n: u16) -> u16 {
        match self.side {
            Side::Front => n,
            Side::Rear => (self.ranks() - 1) * self.files + n,
            Side::Flank if self.last_file => (n * self.files + self.files - 1).min(self.models - 1),
            Side::Flank => n * self.files,
        }
    }
}

/// Where soldier `k` of regiment `i` stands.
fn slot_pos(regs: &Regiments, units: &[UnitStats], i: usize, k: u16) -> FxVec2 {
    let (fs, rs) = units[regs.unit[i] as usize].spacing(regs.formation[i]);
    let local = formation::slot_offset(k, regs.files[i], regs.models[i], fs, rs);
    formation::local_to_world(regs.position[i], regs.facing[i], local)
}

/// Whether regiment `i`'s flank by its last file is nearer `point` than
/// the one by its first.
fn last_file_nearer(regs: &Regiments, units: &[UnitStats], i: usize, point: FxVec2) -> bool {
    let files = regs.files[i].clamp(1, regs.models[i].max(1));
    let first = slot_pos(regs, units, i, 0);
    let last = slot_pos(regs, units, i, files - 1);
    (last - point).length_sq_raw() < (first - point).length_sq_raw()
}

/// The soldiers along one side of a regiment's block, laid out on the
/// ground, for finding who stands across from a point.
struct Line {
    edge: Edge,
    /// The first soldier's spot, and the way to the last one's.
    a: FxVec2,
    along: FxVec2,
}

impl Line {
    fn of(regs: &Regiments, units: &[UnitStats], j: usize, side: Side, last_file: bool) -> Line {
        let edge = Edge::of(regs, j, side, last_file);
        let a = slot_pos(regs, units, j, edge.slot(0));
        let b = slot_pos(regs, units, j, edge.slot(edge.len().max(1) - 1));
        Line {
            edge,
            a,
            along: b - a,
        }
    }

    /// The soldier across from the point `p`: the one nearest where `p`
    /// falls on the line through the side's ends.
    fn across_from(&self, p: FxVec2) -> u16 {
        let n = self.edge.len();
        if n <= 1 {
            return self.edge.slot(0);
        }
        let (along, from_a) = (self.along, p - self.a);
        let dot = i64::from(from_a.x.raw()) * i64::from(along.x.raw())
            + i64::from(from_a.y.raw()) * i64::from(along.y.raw());
        let num = dot * i64::from(n - 1);
        let den = (along.length_sq_raw() as i64).max(1);
        let at = ((2 * num + den) / (2 * den)).clamp(0, i64::from(n - 1));
        self.edge.slot(at as u16)
    }
}

/// Regiment `fight.i`'s turn in melee: each of its soldiers who can reach
/// the foe strikes the soldier across from him on the foe's side that
/// faces it. The front rank fights to the front (and the second rank too,
/// at [`crate::SECOND_RANK_PERCENT`] of a blow, for spears and pikes), the
/// man at the end of each rank to a flank, and the rear rank's men, as many
/// as half the front, to the rear (no more than stand in the rear rank). A blow lands with the chance to hit, and
/// then takes its damage, rolled ±25%, off its man; he dies when his hit
/// points run out, and the soldier in the last slot steps up into his
/// place and takes the rest of the blow. Blows are told to `blows`, while
/// it has room.
fn duel(
    regs: &mut Regiments,
    units: &[UnitStats],
    nav: &Nav,
    fight: Fight,
    rng: &mut Pcg32,
    mut blows: Option<&mut Vec<Blow>>,
) {
    let Fight {
        i,
        j,
        my_side,
        side,
        hit,
        landed,
        reach,
        width,
    } = fight;
    struck_on(regs, j, side);
    let models = regs.models[i];
    let mine = Edge::of(
        regs,
        i,
        my_side,
        last_file_nearer(regs, units, i, regs.position[j]),
    );
    let theirs_last_file = last_file_nearer(regs, units, j, regs.position[i]);
    let ranks_before = formation::ranks(regs.files[j], regs.models[j]);
    // The foe's side facing us, laid out again only when a man falls.
    let mut line = Line::of(regs, units, j, side, theirs_last_file);
    let striker = RegimentHandle {
        index: i as u32,
        generation: regs.generation[i],
    };
    let target = RegimentHandle {
        index: j as u32,
        generation: regs.generation[j],
    };
    // Who strikes: the first `second` of them with a whole blow each.
    let front = mine.files.min(models).min(width);
    let (count, second) = match my_side {
        Side::Front if reach => (front + front.min(models - front), front),
        Side::Front => (front, front),
        Side::Flank => (mine.len(), u16::MAX),
        Side::Rear => (front.div_ceil(2).min(mine.len()), u16::MAX),
    };
    for n in 0..count {
        if regs.models[j] == 0 {
            break;
        }
        let (k, share) = match my_side {
            Side::Rear => (models - 1 - n, 100),
            _ if n >= second => (n, crate::SECOND_RANK_PERCENT),
            _ => (mine.slot(n), 100),
        };
        let t = line.across_from(slot_pos(regs, units, i, k));
        let outcome = if rng.below(100) >= hit {
            Outcome::Parried
        } else {
            let damage = carry(regs, j, roll(landed * share / 100, rng));
            match regs.wound(j, t, damage.min(u64::from(u16::MAX)) as u16) {
                // Too light to cut him, this time: it glanced off.
                0 if damage == 0 => Outcome::Parried,
                0 => Outcome::Wounded,
                men => {
                    if regs.models[j] > 0 {
                        line = Line::of(regs, units, j, side, theirs_last_file);
                    }
                    Outcome::Killed { men: men as u8 }
                }
            }
        };
        if let Some(list) = blows.as_deref_mut()
            && list.len() < list.capacity()
        {
            list.push(Blow {
                striker,
                striker_slot: k as u8,
                target,
                target_slot: t as u8,
                outcome,
            });
        }
    }
    step_up(regs, units, nav, j, ranks_before);
}

/// A regiment that has lost whole ranks (it had `ranks_before`) steps
/// forward, so its front stays where it was (in melee, in contact). A
/// broken one, cut down as it runs, has no line to hold and stays put.
fn step_up(regs: &mut Regiments, units: &[UnitStats], nav: &Nav, j: usize, ranks_before: u16) {
    let ranks_after = formation::ranks(regs.files[j], regs.models[j]);
    if regs.models[j] > 0
        && ranks_after < ranks_before
        && !regs.morale_state[j].broken()
        && !regs.on_wall[j]
    {
        let (_, spacing) = units[regs.unit[j] as usize].spacing(regs.formation[j]);
        let step = Fx::from_raw(spacing.raw() * i32::from(ranks_before - ranks_after) / 2);
        let to = regs.position[j] + regs.facing[j].to_vec() * step;
        if !nav.blocks(regs.owner[j], regs.position[j], to) {
            regs.position[j] = to;
        }
    }
}
