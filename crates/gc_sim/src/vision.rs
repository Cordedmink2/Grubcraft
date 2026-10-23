//! Hiding, spotting and springing ambushes (docs/20-ambushes.md).
//!
//! On a map with cover ([`crate::terrain::Cover`]), every tick works out
//! which parties see each regiment:
//! - its own side and its allies always do;
//! - in the open, in melee, or within [`VOLLEY_REVEAL_TICKS`] of loosing a
//!   volley, everyone does;
//! - with most of its block (four of five points: the centre and the
//!   corners) in a wood, it is seen only by parties with a regiment within
//!   [`WOOD_STILL`] tiles of it, edge to edge ([`WOOD_MOVING`] while it
//!   marches); in tall grass, standing still, within [`GRASS_STILL`];
//! - once seen, it stays seen for [`LINGER_TICKS`] after the last reason.
//!
//! A regiment that makes melee contact with an enemy whose side first saw
//! it less than [`SURPRISE_TICKS`] before springs an **ambush**: the victim
//! loses [`AMBUSH_SHOCK`] morale, is caught off guard for
//! [`OFF_GUARD_TICKS`] (its blows deal [`OFF_GUARD_DAMAGE_PERCENT`]), and
//! its friends within [`RIPPLE_TILES`] lose [`AMBUSH_RIPPLE`]. An ordered
//! volley from hiding costs its target [`VOLLEY_SHOCK`]. Each hidden
//! regiment springs one ambush: after it, the victim's side has it fully in
//! view.
//!
//! Combat asks [`Vision::sees`] before an archer picks a target, holds a
//! hidden archer's fire, and lists the volleys loosed each tick for the
//! volley check; morale (being outnumbered, a router's flight and rally)
//! counts only seen enemies; an attack order needs a seen target, and a
//! chase after a target that goes out of sight becomes a march to where it
//! was last seen.
//!
//! On a map with no cover all of this is switched off: everyone sees
//! everything, nothing is a surprise, nothing is stored or hashed.
//!
//! **Fog of war** (a Conquest match played for a victory, see
//! [`crate::World::conquest`]) adds sight on top. A party sees a regiment
//! only while one of its side's regiments is within [`SIGHT`] of it, or one
//! of its side's buildings within that building's
//! [`sight`](crate::structures::Role::sight), edge to edge; cover still
//! hides it within that, and buildings spot into cover as regiments do.
//! Coming into sight is no surprise: only cover springs ambushes. A
//! building is known to a party once seen, and stays known.
//!
//! Sightings are kept for [`MAX_PARTIES`](crate::MAX_PARTIES) parties,
//! the combat grid's owner slots; the sim refuses regiments for parties
//! past that.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::combat::{Block, Combat, Whose, placement};
use crate::morale;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::Structures;
use crate::terrain::{Cover, Terrain};
use crate::{PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Parties whose sight is tracked.
pub const PARTIES: usize = crate::MAX_PARTIES;
// Ally masks are a byte per party: widen them before raising the limit.
const _: () = assert!(PARTIES <= 8);
/// How far a regiment sees in fog of war, edge to edge, in tiles: an
/// archer's reach and a little more.
pub const SIGHT: Fx = Fx::from_int(32);
/// Spotting distances, edge to edge, in tiles.
pub const GRASS_STILL: Fx = Fx::from_int(3);
pub const WOOD_STILL: Fx = Fx::from_int(4);
pub const WOOD_MOVING: Fx = Fx::from_int(10);
/// How long a regiment stays seen after the last reason to see it.
pub const LINGER_TICKS: u8 = 3 * TICKS_PER_SECOND as u8;

/// In fog of war, each regiment and building looks round it once in this
/// many ticks (each on its own tick), not every tick: sight is the
/// costliest search in a big match, and a sighting lingers anyway. A
/// stopgap until sight is kept on per-player tile grids, stamped only as
/// things cross tiles (docs/10-architecture.md, "Fog of war").
pub const SIGHT_EVERY: u32 = 4;

/// Whether the thing in `slot` looks round it on `tick` (see
/// [`SIGHT_EVERY`]).
fn looks_now(slot: u32, tick: u32) -> bool {
    slot.wrapping_add(tick).is_multiple_of(SIGHT_EVERY)
}
/// How long loosing a volley keeps a regiment seen by everyone.
pub const VOLLEY_REVEAL_TICKS: u8 = 5 * TICKS_PER_SECOND as u8;
/// Contact made less than this long after the victim's side first saw the
/// attacker springs an ambush.
pub const SURPRISE_TICKS: u16 = 4 * TICKS_PER_SECOND as u16;
/// Morale points the victim of an ambush loses at once.
pub const AMBUSH_SHOCK: i16 = 20;
/// Morale points the victim's friends within [`RIPPLE_TILES`] lose.
pub const AMBUSH_RIPPLE: i16 = 5;
pub const RIPPLE_TILES: i32 = 16;
/// Morale points the target of an ordered volley from hiding loses.
pub const VOLLEY_SHOCK: i16 = 10;
/// How long an ambush's victim is caught off guard, and the damage its
/// blows deal meanwhile, percent.
pub const OFF_GUARD_TICKS: u8 = 6 * TICKS_PER_SECOND as u8;
pub const OFF_GUARD_DAMAGE_PERCENT: u64 = 70;
/// Ambushes remembered per tick, for the game's alerts.
pub const MAX_AMBUSHES: usize = 64;

/// An ambush sprung this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ambush {
    pub ambusher: RegimentHandle,
    pub victim: RegimentHandle,
    /// Sprung with a volley rather than a charge.
    pub volley: bool,
}

/// A building as a watcher in fog of war.
#[derive(Clone, Copy)]
struct Post {
    slot: usize,
    owner: PlayerId,
    block: Block,
    sight: Fx,
}

/// Who sees whom. All storage is reserved when the match starts.
pub(crate) struct Vision {
    /// The map has cover or the match is in fog of war; otherwise none of
    /// this runs.
    active: bool,
    /// Per party: the parties that share its sight (itself and its allies).
    allies: [u8; PARTIES],
    /// Per slot: the generation of the regiment the state below belongs
    /// to, if any (a new one in the slot starts afresh).
    tracked: Vec<Option<u16>>,
    /// Per slot and party: ticks the party has had the regiment in view,
    /// counting up to [`SURPRISE_TICKS`]; 0 is unseen.
    sighting: Vec<u16>,
    /// Per slot and party: ticks a sighting lingers after the last reason.
    linger: Vec<u8>,
    /// Per slot: ticks a volley keeps it seen by everyone.
    revealed: Vec<u8>,
    /// Per slot: ticks it is still caught off guard.
    off_guard: Vec<u8>,
    /// Per slot: in melee at the end of the last tick.
    was_engaged: Vec<bool>,
    /// Ambushes sprung this tick.
    ambushes: Vec<Ambush>,
    /// Fog of war: sight decides who can look at all.
    fog: bool,
    /// Per slot, in fog of war: the parties with it in sight, or lately
    /// so.
    in_sight: Vec<u8>,
    /// Per slot and party, in fog of war: ticks it stays in sight after
    /// the party's last watcher loses it, so sight does not flicker at the
    /// edge.
    sight_linger: Vec<u8>,
    /// Per slot, in fog of war: placed and not yet looked round, so it
    /// looks on its first tick whatever its turn (see [`SIGHT_EVERY`]).
    unlooked: Vec<bool>,
    /// In fog of war, this tick: each building's owner, footprint and
    /// sight.
    posts: Vec<Post>,
    /// The same posts by where they stand, in square cells
    /// [`POST_CELL`] tiles across, row by row: cell `k`'s posts are
    /// `by_cell[cell_start[k]..cell_start[k + 1]]`.
    by_cell: Vec<Post>,
    cell_start: Vec<u32>,
    /// Cells a side.
    cells: usize,
    /// The furthest any post sees, and the largest post's radius.
    post_sight: Fx,
    post_radius: Fx,
}

/// The side of a cell posts are sorted into, in tiles.
const POST_CELL: i32 = 32;

/// The cell (column, row) of a map `cells` cells across that a point is
/// sorted into; points off the map go to the nearest cell.
fn post_cell(cells: usize, p: FxVec2) -> (usize, usize) {
    let at = |v: Fx| (v.floor_int() / POST_CELL).clamp(0, cells as i32 - 1) as usize;
    (at(p.x), at(p.y))
}

impl Vision {
    pub(crate) fn new(capacity: usize, terrain: &Terrain, stances: &Stances) -> Vision {
        let active = terrain.has_cover();
        let n = if active { capacity } else { 0 };
        let mut allies = [0u8; PARTIES];
        for (p, mask) in allies.iter_mut().enumerate() {
            for q in 0..PARTIES {
                if !stances.hostile(p as PlayerId, q as PlayerId) {
                    *mask |= 1 << q;
                }
            }
        }
        let mut v = Vision {
            active,
            allies,
            tracked: Vec::new(),
            sighting: Vec::new(),
            linger: Vec::new(),
            revealed: Vec::new(),
            off_guard: Vec::new(),
            was_engaged: Vec::new(),
            ambushes: Vec::new(),
            fog: false,
            in_sight: Vec::new(),
            sight_linger: Vec::new(),
            unlooked: Vec::new(),
            posts: Vec::new(),
            by_cell: Vec::new(),
            cell_start: Vec::new(),
            cells: 0,
            post_sight: Fx::ZERO,
            post_radius: Fx::ZERO,
        };
        if active {
            v.reserve(n);
        }
        v
    }

    /// Room for `capacity` regiments' sightings.
    fn reserve(&mut self, capacity: usize) {
        self.tracked = vec![None; capacity];
        self.sighting = vec![0; capacity * PARTIES];
        self.linger = vec![0; capacity * PARTIES];
        self.revealed = vec![0; capacity];
        self.off_guard = vec![0; capacity];
        self.was_engaged = vec![false; capacity];
        self.ambushes = Vec::with_capacity(MAX_AMBUSHES);
    }

    /// Play in fog of war from now on, with room for `capacity` regiments
    /// and `buildings` buildings, on a map `map_tiles` across. Only before
    /// the first tick.
    pub(crate) fn fog(&mut self, capacity: usize, buildings: usize, map_tiles: u32) {
        if !self.active {
            self.active = true;
            self.reserve(capacity);
        }
        self.fog = true;
        self.in_sight = vec![0; capacity];
        self.sight_linger = vec![0; capacity * PARTIES];
        self.unlooked = vec![false; capacity];
        self.posts = Vec::with_capacity(buildings);
        self.by_cell = Vec::with_capacity(buildings);
        self.cells = (map_tiles as usize).div_ceil(POST_CELL as usize).max(1);
        self.cell_start = vec![0; self.cells * self.cells + 1];
    }

    pub(crate) fn fogged(&self) -> bool {
        self.fog
    }

    /// In fog of war, as of the last tick: each standing building's owner,
    /// footprint and sight.
    pub(crate) fn posts(&self) -> impl Iterator<Item = (PlayerId, Block, Fx)> + '_ {
        self.posts.iter().map(|p| (p.owner, p.block, p.sight))
    }

    /// The parties that share `party`'s sight: itself and its allies.
    pub(crate) fn allies_of(&self, party: PlayerId) -> u8 {
        self.allies.get(usize::from(party)).copied().unwrap_or(0)
    }

    /// Whether `party` sees the regiment in slot `i`, owned by `owner`: in
    /// sight, and not hidden by cover.
    pub(crate) fn sees(&self, party: PlayerId, i: usize, owner: PlayerId) -> bool {
        self.uncovered(party, i, owner)
            && (!self.fog || self.in_sight[i] & (1 << party) != 0 || self.friends(party, owner))
    }

    /// Whether cover leaves the regiment in slot `i`, owned by `owner`, open
    /// to `party` (who may still not have it in sight in fog of war).
    pub(crate) fn uncovered(&self, party: PlayerId, i: usize, owner: PlayerId) -> bool {
        self.friends(party, owner) || self.sighting[i * PARTIES + party as usize] > 0
    }

    /// Whether `party` sees all of `owner`'s regiments, being a friend (or
    /// there being nothing to hide them).
    fn friends(&self, party: PlayerId, owner: PlayerId) -> bool {
        !self.active
            || party as usize >= PARTIES
            || owner as usize >= PARTIES
            || self.allies[owner as usize] & (1 << party) != 0
    }

    /// Whether cover hides the regiment in slot `i` from every party hostile
    /// to its owner (out of sight in fog of war is not hidden).
    pub(crate) fn hidden(&self, i: usize, owner: PlayerId) -> bool {
        if !self.active || owner as usize >= PARTIES {
            return false;
        }
        // Cover hides; being out of sight in fog of war is not hiding.
        let friends = self.allies[owner as usize];
        (0..PARTIES).all(|p| friends & (1 << p) != 0 || self.sighting[i * PARTIES + p] == 0)
    }

    /// Whether the regiment in slot `i` is caught off guard by an ambush.
    pub(crate) fn off_guard(&self, i: usize) -> bool {
        self.active && self.off_guard[i] > 0
    }

    /// Ambushes sprung in the last tick.
    pub fn ambushes(&self) -> &[Ambush] {
        &self.ambushes
    }

    /// Work out who sees whom after this tick's moves and blows, spring
    /// any ambushes, and end chases after targets gone out of sight.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        structures: Option<&mut Structures>,
        tick: u32,
    ) {
        if !self.active {
            return;
        }
        self.ambushes.clear();
        self.posts.clear();
        if self.fog
            && let Some(s) = &structures
        {
            self.posts.extend(s.standing().map(|slot| Post {
                slot,
                owner: s.owner_of(slot),
                block: s.block(slot),
                sight: s.sight(slot),
            }));
            self.sort_posts();
        }
        // Timers, and regiments new to their slots.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                self.tracked[i] = None;
                continue;
            }
            if self.tracked[i] != Some(regs.generation[i]) {
                self.start(regs, units, terrain, i);
            }
            self.revealed[i] = self.revealed[i].saturating_sub(1);
            self.off_guard[i] = self.off_guard[i].saturating_sub(1);
        }

        // Ambushes, judged on what each side saw before this tick: volleys
        // loosed from hiding, then charges out of it.
        // (A tower that shoots is known to its target's side as it looses,
        // see [`crate::towers`].)
        for &(_, shooter, target) in combat.loosed() {
            let Some(i) = shooter.regiment().and_then(|h| regs.resolve(h)) else {
                continue;
            };
            self.revealed[i] = VOLLEY_REVEAL_TICKS;
            if let Some(j) = regs.resolve(target)
                && self.surprises(i, regs.owner[j])
            {
                self.spring(regs, combat, stances, i, j, true);
            }
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let engaged = regs.foe(i);
            if let Some(j) = engaged
                && !self.was_engaged[i]
                && self.surprises(i, regs.owner[j])
            {
                self.spring(regs, combat, stances, i, j, false);
            }
            self.was_engaged[i] = engaged.is_some();
        }

        // Who sees whom now.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let owner = regs.owner[i];
            let friends = (self.allies.get(owner as usize))
                .copied()
                .unwrap_or(u8::MAX);
            let cover = self.spotting(regs, units, terrain, i);
            let fog_now = self.fog && (looks_now(i as u32, tick) || self.unlooked[i]);
            let (near, far) = match owner as usize {
                o if o < PARTIES => {
                    let far = fog_now.then_some(SIGHT);
                    self.spotters(regs, combat, units, stances, i, cover, far)
                }
                _ => (u8::MAX, u8::MAX),
            };
            let direct = if cover.is_some() {
                friends | near
            } else {
                u8::MAX
            };
            if fog_now {
                let now = friends | far;
                let mut in_sight = 0;
                for p in 0..PARTIES {
                    let k = i * PARTIES + p;
                    if now & (1 << p) != 0 {
                        self.sight_linger[k] = LINGER_TICKS;
                    } else {
                        self.sight_linger[k] =
                            self.sight_linger[k].saturating_sub(SIGHT_EVERY as u8);
                    }
                    if self.sight_linger[k] > 0 {
                        in_sight |= 1 << p;
                    }
                }
                self.in_sight[i] = in_sight;
                self.unlooked[i] = false;
            }
            for p in 0..PARTIES {
                let k = i * PARTIES + p;
                if direct & (1 << p) != 0 {
                    self.linger[k] = LINGER_TICKS;
                } else if self.linger[k] > 0 {
                    self.linger[k] -= 1;
                } else {
                    self.sighting[k] = 0;
                    continue;
                }
                self.sighting[k] = (self.sighting[k] + 1).min(SURPRISE_TICKS);
            }
        }

        if let Some(s) = structures {
            self.look_at_buildings(regs, combat, units, stances, s, tick);
        }

        // A chase after a target its side no longer sees ends where the
        // target was last seen.
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let Some(j) = regs.attack_target[i].and_then(|h| regs.resolve(h)) else {
                continue;
            };
            if self.sees(regs.owner[i], j, regs.owner[j]) {
                continue;
            }
            regs.attack_target[i] = None;
            if regs.target[i].is_some() {
                regs.target[i] = Some(regs.position[j]);
            }
        }
    }

    /// Start tracking the regiment just placed in slot `i`: seen by
    /// everyone if it stands in the open (so a regiment placed in plain
    /// view is no surprise), by its own side only if it starts in cover.
    pub(crate) fn start(
        &mut self,
        regs: &Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
        i: usize,
    ) {
        if !self.active {
            return;
        }
        self.tracked[i] = Some(regs.generation[i]);
        if self.fog {
            self.in_sight[i] = (self.allies.get(regs.owner[i] as usize))
                .copied()
                .unwrap_or(u8::MAX);
            self.sight_linger[i * PARTIES..(i + 1) * PARTIES].fill(0);
            self.unlooked[i] = true;
        }
        self.revealed[i] = 0;
        self.off_guard[i] = 0;
        self.was_engaged[i] = false;
        let in_open = self.spotting(regs, units, terrain, i).is_none();
        let friends = self
            .allies
            .get(regs.owner[i] as usize)
            .copied()
            .unwrap_or(u8::MAX);
        for p in 0..PARTIES {
            let k = i * PARTIES + p;
            self.sighting[k] = if in_open || friends & (1 << p) != 0 {
                SURPRISE_TICKS
            } else {
                0
            };
            self.linger[k] = 0;
        }
    }

    /// Let every party have seen the regiment in slot `i` for a while, so
    /// it is no surprise to anyone (it stays seen while a reason holds).
    pub(crate) fn show(&mut self, i: usize) {
        if !self.active {
            return;
        }
        let k = i * PARTIES;
        self.sighting[k..k + PARTIES].fill(SURPRISE_TICKS);
        self.linger[k..k + PARTIES].fill(LINGER_TICKS);
    }

    /// Give slot `i` what `from`, a world on the same map, knows about the
    /// regiment in it: who has seen it and for how long.
    pub(crate) fn copy_from(&mut self, from: &Vision, i: usize) {
        if !self.active {
            return;
        }
        self.tracked[i] = from.tracked[i];
        let k = i * PARTIES;
        self.sighting[k..k + PARTIES].copy_from_slice(&from.sighting[k..k + PARTIES]);
        self.linger[k..k + PARTIES].copy_from_slice(&from.linger[k..k + PARTIES]);
        self.revealed[i] = from.revealed[i];
        self.off_guard[i] = from.off_guard[i];
        self.was_engaged[i] = from.was_engaged[i];
        if self.fog && from.fog {
            self.in_sight[i] = from.in_sight[i];
            self.sight_linger[k..k + PARTIES].copy_from_slice(&from.sight_linger[k..k + PARTIES]);
            self.unlooked[i] = from.unlooked[i];
        }
    }

    /// Whether the regiment in slot `i` takes `party` by surprise: that
    /// party first saw it less than [`SURPRISE_TICKS`] ago.
    fn surprises(&self, i: usize, party: PlayerId) -> bool {
        party < PARTIES as u8 && self.sighting[i * PARTIES + party as usize] < SURPRISE_TICKS
    }

    /// Regiment `i` springs an ambush on regiment `j`.
    fn spring(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        stances: &Stances,
        i: usize,
        j: usize,
        volley: bool,
    ) {
        if volley {
            morale::shift(regs, j, -VOLLEY_SHOCK);
        } else {
            morale::shift(regs, j, -AMBUSH_SHOCK);
            self.off_guard[j] = OFF_GUARD_TICKS;
            morale::shake_friends(regs, combat, stances, j, RIPPLE_TILES, AMBUSH_RIPPLE);
        }
        if self.ambushes.len() < MAX_AMBUSHES {
            self.ambushes.push(Ambush {
                ambusher: regs.handle(i),
                victim: regs.handle(j),
                volley,
            });
        }
        // Sprung: the victim's side knows all about it now, so it springs
        // one ambush, not one per blow or volley.
        let friends = self.allies[regs.owner[j] as usize];
        for p in 0..PARTIES {
            if friends & (1 << p) != 0 {
                self.sighting[i * PARTIES + p] = SURPRISE_TICKS;
                self.linger[i * PARTIES + p] = LINGER_TICKS;
            }
        }
    }

    /// How close, edge to edge, an enemy must come to see regiment `i`, or
    /// `None` when everyone sees it: it stands in the open, fights, or has
    /// just loosed a volley.
    fn spotting(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
        i: usize,
    ) -> Option<Fx> {
        if regs.engaged[i].is_some() || self.revealed[i] > 0 {
            return None;
        }
        let b = Block::of(regs, units, i);
        let (c, across, along) = (b.centre, b.right * b.half_width, b.forward * b.half_depth);
        let (mut wood, mut covered) = (0, 0);
        for p in [
            c,
            c + across + along,
            c + across - along,
            c - across + along,
            c - across - along,
        ] {
            match terrain.cover(p) {
                Cover::Wood => {
                    wood += 1;
                    covered += 1;
                }
                Cover::Grass => covered += 1,
                Cover::Open => {}
            }
        }
        let moving = regs.target[i].is_some();
        if wood >= 4 {
            Some(if moving { WOOD_MOVING } else { WOOD_STILL })
        } else if covered >= 4 && !moving {
            Some(GRASS_STILL)
        } else {
            None
        }
    }

    /// The parties with a regiment (theirs or an ally's), or in fog of war
    /// a building, within `near` of regiment `i`, edge to edge, as a bit
    /// set; and the same within `far`, both in one search (none for a
    /// distance not given). A building looks no further than its own sight.
    #[allow(clippy::too_many_arguments)]
    fn spotters(
        &self,
        regs: &Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        i: usize,
        near: Option<Fx>,
        far: Option<Fx>,
    ) -> (u8, u8) {
        let (mut seen_near, mut seen_far) = (0u8, 0u8);
        let Some(reach) = near.max(far) else {
            return (0, 0);
        };
        let at = placement!(regs, stances);
        let me = Block::of(regs, units, i);
        let looks = |seen: u8, party: usize, them: &Block, d: Option<Fx>| {
            seen & (1 << party) == 0 && d.is_some_and(|d| me.touches(them, d))
        };
        combat.each_near(
            &at,
            me.centre,
            reach + me.radius() + combat.max_radius(),
            Whose::Enemies(regs.owner[i]),
            |j| {
                let party = regs.owner[j] as usize;
                if party >= PARTIES || seen_near & seen_far & (1 << party) != 0 {
                    return;
                }
                let them = Block::of(regs, units, j);
                if looks(seen_near, party, &them, near) {
                    seen_near |= self.allies[party];
                }
                if looks(seen_far, party, &them, far) {
                    seen_far |= self.allies[party];
                }
            },
        );
        let posts = |d: Option<Fx>| d.map_or(0, |d| self.posts_near(&me, regs.owner[i], d));
        (seen_near | posts(near), seen_far | posts(far))
    }

    /// Sort this tick's posts into their cells (a counting sort into room
    /// reserved at the start).
    fn sort_posts(&mut self) {
        let cells = self.cells;
        let cell = |p: &Post| {
            let (x, y) = post_cell(cells, p.block.centre);
            y * cells + x
        };
        // Count each cell's posts, sum them so each cell holds where its
        // posts end, then fill backwards, leaving where each one starts.
        self.cell_start.fill(0);
        (self.post_sight, self.post_radius) = (Fx::ZERO, Fx::ZERO);
        for p in &self.posts {
            self.cell_start[cell(p)] += 1;
            self.post_sight = self.post_sight.max(p.sight);
            self.post_radius = self.post_radius.max(p.block.radius());
        }
        for k in 1..self.cell_start.len() {
            self.cell_start[k] += self.cell_start[k - 1];
        }
        self.by_cell.clear();
        self.by_cell.extend_from_slice(&self.posts);
        for p in self.posts.iter().rev() {
            let k = cell(p);
            self.cell_start[k] -= 1;
            self.by_cell[self.cell_start[k] as usize] = *p;
        }
    }

    /// The parties hostile to `owner` with a building within `distance` of
    /// `me` (and within its own sight), edge to edge.
    fn posts_near(&self, me: &Block, owner: PlayerId, distance: Fx) -> u8 {
        if self.by_cell.is_empty() {
            return 0;
        }
        let mut seen = 0u8;
        // Only cells a post seeing `me` could stand in.
        let reach = distance.min(self.post_sight) + me.radius() + self.post_radius;
        let (x0, y0) = post_cell(self.cells, me.centre - FxVec2::new(reach, reach));
        let (x1, y1) = post_cell(self.cells, me.centre + FxVec2::new(reach, reach));
        for y in y0..=y1 {
            let row = y * self.cells;
            let (from, to) = (self.cell_start[row + x0], self.cell_start[row + x1 + 1]);
            for post in &self.by_cell[from as usize..to as usize] {
                let p = post.owner as usize;
                if p >= PARTIES || seen & (1 << p) != 0 || self.allies[p] & (1 << owner) != 0 {
                    continue;
                }
                // Far off by the centres: no need for the block test.
                let gap = distance.min(post.sight);
                let reach = u64::from(
                    (gap + me.radius() + post.block.radius())
                        .raw()
                        .unsigned_abs(),
                );
                if (post.block.centre - me.centre).length_sq_raw() <= reach * reach
                    && me.touches(&post.block, gap)
                {
                    seen |= self.allies[p];
                }
            }
        }
        seen
    }

    /// Mark who has now seen each building: its side, and any party with a
    /// regiment within [`SIGHT`] of it or a building within its sight. Each
    /// building looks once in [`SIGHT_EVERY`] ticks.
    fn look_at_buildings(
        &self,
        regs: &Regiments,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
        structures: &mut Structures,
        tick: u32,
    ) {
        let at = placement!(regs, stances);
        // The parties in play: once all of them have seen a building there
        // is nothing more to look for.
        let present = (0..regs.alive.len())
            .filter(|&i| regs.alive[i])
            .map(|i| regs.owner[i])
            .chain(self.posts.iter().map(|p| p.owner))
            .filter(|&p| (p as usize) < PARTIES)
            .fold(0u8, |m, p| m | 1 << p);
        for post in &self.posts {
            let o = post.owner as usize;
            if o >= PARTIES
                || !looks_now(post.slot as u32, tick)
                || structures.seen(post.slot) & present == present
            {
                continue;
            }
            let block = post.block;
            let mut seen = self.allies[o] | structures.seen(post.slot);
            combat.each_near(
                &at,
                block.centre,
                SIGHT + block.radius() + combat.max_radius(),
                Whose::Enemies(post.owner),
                |j| {
                    let party = regs.owner[j] as usize;
                    if party < PARTIES
                        && seen & (1 << party) == 0
                        && block.touches(&Block::of(regs, units, j), SIGHT)
                    {
                        seen |= self.allies[party];
                    }
                },
            );
            // Each building looks as far as its own sight.
            seen |= self.posts_near(&block, post.owner, Fx::MAX);
            structures.mark_seen(post.slot, seen);
        }
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        if !self.active {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || self.tracked[i].is_none() {
                continue;
            }
            for k in i * PARTIES..(i + 1) * PARTIES {
                h.u16(self.sighting[k]);
                h.u8(self.linger[k]);
            }
            h.u8(self.revealed[i]);
            h.u8(self.off_guard[i]);
            h.u8(u8::from(self.was_engaged[i]));
            if self.fog {
                h.u8(self.in_sight[i]);
                for k in i * PARTIES..(i + 1) * PARTIES {
                    h.u8(self.sight_linger[k]);
                }
                h.u8(u8::from(self.unlooked[i]));
            }
        }
    }
}
