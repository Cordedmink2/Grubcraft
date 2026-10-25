//! Manning the walls (docs/26-fortifications.md, "Manning the walls"): a
//! regiment sent onto its side's wall of level 2 or 3, or its gatehouse,
//! walks to the nearest stair (a tower or gate joined to the wall, or an
//! end of it), climbs, and spreads along the walkway in one rank, facing
//! out. A gatehouse is part of the walkway of the raised wall it touches,
//! with room for three soldiers.
//!
//! Up there it is off the walking grid ([`crate::regiments`] `on_wall`):
//! the wall holds it where it stands, melee from the ground can't reach
//! it (nor it the ground), its shooters reach [`WALL_RANGE_PERCENT`] as
//! far and arrows at it do [`WALL_COVER_PERCENT`] of their damage. Any
//! other order brings it down at the foot it climbed from. When a tile it
//! holds falls (or a Choir wall under it loses power) the whole regiment
//! comes down with it into the breach, hurt and in disorder.
//!
//! Foot and archers of an enemy can climb a walkway on ladders
//! ([`LADDER_TICKS`]). Where a defender holds the tile they climbed to,
//! they fight it at the ladder head ([`Regiments`] `climber`): head on, at
//! most [`LADDER_FRONT`] wide, dealing [`LADDER_DAMAGE_PERCENT`] of their
//! damage and taking [`REACH_VS_LADDER_PERCENT`] from spears. Once no
//! defender holds it they spread along the walkway like a regiment
//! manning it, facing in, and come down inside the walls.

use crate::Formation;
use crate::combat::Block;
use crate::conquest::Conquest;
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments};
use crate::stances::Stances;
use crate::structures::{Refusal, Role, StructureHandle, Work, footprint_centre};
use crate::veins::foot_reaches;
use crate::{PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats, Vec};
use gc_fixed::{Angle, Fx, FxVec2};

/// How far shooters on a wall reach, in percent of their range on the
/// ground.
pub const WALL_RANGE_PERCENT: i32 = 120;
/// What arrows do to those on a wall, in percent of their damage.
pub const WALL_COVER_PERCENT: u64 = 50;
/// The most wall tiles one regiment holds.
pub const MAX_HELD: usize = 24;
/// The most regiments on (or making for) the walls at once, every side
/// together.
pub const MAX_HOLDS: usize = 256;
/// Ticks a regiment takes to climb at the foot of the wall.
pub const CLIMB_TICKS: u16 = 3 * TICKS_PER_SECOND as u16;
/// Ticks a regiment takes to climb an enemy's wall on ladders.
pub const LADDER_TICKS: u16 = 3 * CLIMB_TICKS;
/// The most soldiers a side fights with at the head of a ladder.
pub const LADDER_FRONT: u16 = 3;
/// What climbers' blows do at the head of a ladder, in percent.
pub const LADDER_DAMAGE_PERCENT: u64 = 50;
/// What spears and pikes do to climbers, in percent.
pub const REACH_VS_LADDER_PERCENT: u64 = 150;
/// Hit points each soldier on a tile that falls loses.
pub const FALL_HP: u32 = 10;
/// How far along a wall a regiment looks for its stair, in tiles.
const STAIR_TILES: usize = 128;

/// Where a regiment sent onto a wall is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Walking to the stair (its [`Work::Man`] job), or to the foot of an
    /// enemy's wall ([`Work::Climb`]).
    Going,
    /// Climbing at the foot: ticks left.
    Climbing(u16),
    /// Up on the walkway.
    Holding,
}

/// One regiment on, or making for, a wall.
#[derive(Clone, Copy, Debug)]
struct Hold {
    regiment: RegimentHandle,
    phase: Phase,
    /// Climbing an enemy's wall on ladders, rather than its own.
    ladder: bool,
    /// The tile it was sent to: it spreads out from there.
    tile: StructureHandle,
    /// Where it stood to climb, and comes down to.
    foot: FxVec2,
    /// Where it was when sent: it faces away from there on the wall.
    from: FxVec2,
    /// Its formation and frontage before it climbed, back when it comes
    /// down.
    formation: Formation,
    frontage: u16,
    /// The pieces it holds, the first `held` of them, and the footprint
    /// of each (to come down where one fell).
    held: u8,
    tiles: [StructureHandle; MAX_HELD],
    at: [[i16; 4]; MAX_HELD],
}

/// The regiments on the walls (see the module docs).
#[derive(Clone, Debug, Default)]
pub(crate) struct Manning {
    holds: Vec<Hold>,
    /// Scratch for [`Conquest::walkway_stretch`], kept so a tick never
    /// allocates.
    stretch: Vec<usize>,
    others: Vec<([i32; 4], usize)>,
}

impl Manning {
    /// Room for every hold, and scratch for a match of `structures`
    /// buildings.
    pub(crate) fn new(structures: usize) -> Manning {
        Manning {
            holds: Vec::with_capacity(MAX_HOLDS),
            stretch: Vec::with_capacity(STAIR_TILES),
            others: Vec::with_capacity(structures),
        }
    }

    /// The hold of regiment slot `r`, if it has one.
    fn find(&self, r: usize) -> Option<usize> {
        self.holds
            .iter()
            .position(|h| h.regiment.index as usize == r)
    }

    /// Whether wall slot handle `t` is held by any regiment.
    fn taken(&self, t: StructureHandle) -> bool {
        self.held_by_other(t, usize::MAX)
    }

    /// Whether wall slot handle `t` is held by a regiment other than hold
    /// `k`'s.
    fn held_by_other(&self, t: StructureHandle, k: usize) -> bool {
        (self.holds.iter().enumerate())
            .any(|(n, h)| n != k && h.tiles[..usize::from(h.held)].contains(&t))
    }

    /// The regiment slot holding wall slot handle `t` against hold `k`'s
    /// (another's, hostile to it), if any.
    fn defender(
        &self,
        t: StructureHandle,
        k: usize,
        regs: &Regiments,
        stances: &Stances,
    ) -> Option<usize> {
        let me = regs.owner[self.holds[k].regiment.index as usize];
        (self.holds.iter().enumerate())
            .filter(|&(n, h)| n != k && h.tiles[..usize::from(h.held)].contains(&t))
            .filter_map(|(_, h)| regs.resolve(h.regiment))
            .find(|&j| stances.hostile(me, regs.owner[j]))
    }

    /// The tiles regiment `h` holds, if it is on a wall.
    pub(crate) fn tiles_of(&self, h: RegimentHandle) -> &[StructureHandle] {
        self.holds
            .iter()
            .find(|o| o.regiment == h && o.phase == Phase::Holding)
            .map_or(&[], |o| &o.tiles[..usize::from(o.held)])
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        for o in &self.holds {
            if regs.resolve(o.regiment).is_none() {
                continue;
            }
            h.u32(o.regiment.index);
            h.u16(o.regiment.generation);
            match o.phase {
                Phase::Going => h.u8(0),
                Phase::Climbing(t) => {
                    h.u8(1);
                    h.u16(t);
                }
                Phase::Holding => h.u8(2),
            }
            h.u8(u8::from(o.ladder));
            h.u32(o.tile.index);
            h.u16(o.tile.generation);
            h.i32(o.foot.x.raw());
            h.i32(o.foot.y.raw());
            h.i32(o.from.x.raw());
            h.i32(o.from.y.raw());
            h.u8(o.formation as u8);
            h.u16(o.frontage);
            h.u8(o.held);
            for t in &o.tiles[..usize::from(o.held)] {
                h.u32(t.index);
                h.u16(t.generation);
            }
            for foot in &o.at[..usize::from(o.held)] {
                for &v in foot {
                    h.u16(v as u16);
                }
            }
        }
    }
}

impl Conquest {
    /// Whether regiment slot `r`, `player`'s, may go onto wall tile `h`,
    /// and the tile's slot: `player`'s finished wall with a walkway (level
    /// 2 or 3), powered if it needs power, and a regiment that mans walls
    /// (see [`crate::ConquestUnit::mans_walls`]), steady and not fighting.
    pub(crate) fn man_check(
        &self,
        player: PlayerId,
        r: usize,
        regs: &Regiments,
        h: StructureHandle,
    ) -> Result<usize, Refusal> {
        let i = self.structures.owned(h, player).ok_or(Refusal::Unknown)?;
        self.walkway_check(i, r, regs)
    }

    /// Whether regiment slot `r` may go onto wall slot `i`, whoever's it
    /// is: a finished wall with a walkway, powered if it needs power, and
    /// a regiment that mans walls, steady and not fighting. `i` if so.
    fn walkway_check(&self, i: usize, r: usize, regs: &Regiments) -> Result<usize, Refusal> {
        let s = &self.structures;
        let wall = s.stats_of(i).role == Role::Wall || s.is_gate(i);
        if !wall || !self.unit(regs.unit[r]).mans_walls {
            return Err(Refusal::Unknown);
        }
        if !s.walkway(i) {
            return Err(Refusal::NoWalkway);
        }
        if !s.finished(i) {
            return Err(Refusal::Busy);
        }
        if !s.powered(i) {
            return Err(Refusal::Unpowered);
        }
        if regs.morale_state[r].broken() || regs.engaged[r].is_some() {
            return Err(Refusal::Busy);
        }
        if self.manning.find(r).is_none() && self.manning.holds.len() >= MAX_HOLDS {
            return Err(Refusal::NoRoom);
        }
        Ok(i)
    }

    /// `player` sends regiment slot `r` onto wall tile `h` (see
    /// [`Conquest::man_check`]): off whatever wall it was on, it goes to
    /// the nearest stair to climb.
    pub(crate) fn man_wall(
        &mut self,
        player: PlayerId,
        r: usize,
        h: StructureHandle,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
    ) -> bool {
        if self.man_check(player, r, regs, h).is_err() {
            return false;
        }
        self.set_off(r, h, false, regs, units, nav);
        true
    }

    /// Regiment slot `r` sets off for wall tile `h` (checked): off
    /// whatever wall it was on, it leaves its work and walks to the
    /// nearest stair, or for `ladder`s to the tile itself, a hold going.
    fn set_off(
        &mut self,
        r: usize,
        h: StructureHandle,
        ladder: bool,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
    ) {
        if let Some(k) = self.manning.find(r) {
            let foot = self.manning.holds[k].foot;
            self.come_down(k, foot, regs, units, nav);
        }
        self.stop_work(r);
        let (to, work) = match self.structures.resolve(h) {
            Some(i) if !ladder => (self.stair(i, regs.position[r]), Work::Man),
            Some(_) => (h, Work::Climb),
            None => return,
        };
        self.structures.send(regs, r, to, work);
        self.manning.holds.push(Hold {
            regiment: regs.handle(r),
            phase: Phase::Going,
            ladder,
            tile: h,
            foot: regs.position[r],
            from: regs.position[r],
            formation: regs.formation[r],
            frontage: regs.frontage[r],
            held: 0,
            tiles: [h; MAX_HELD],
            at: [[0; 4]; MAX_HELD],
        });
    }

    /// Whether regiment slot `r` may climb wall tile `h` on ladders, and
    /// the tile's slot: as [`Conquest::man_check`], but anyone's wall (the
    /// caller checks the owner is hostile, and seen).
    pub(crate) fn climb_check(
        &self,
        r: usize,
        regs: &Regiments,
        h: StructureHandle,
    ) -> Result<usize, Refusal> {
        let i = self.structures.resolve(h).ok_or(Refusal::Unknown)?;
        self.walkway_check(i, r, regs)
    }

    /// Regiment slot `r` goes to climb enemy wall tile `h` on ladders (see
    /// [`Conquest::climb_check`]), off whatever wall it was on.
    pub(crate) fn climb_wall(
        &mut self,
        r: usize,
        h: StructureHandle,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
    ) -> bool {
        if self.climb_check(r, regs, h).is_err() {
            return false;
        }
        self.set_off(r, h, true, regs, units, nav);
        true
    }

    /// The walkway joined to wall slot `i` (its owner's, finished and
    /// powered), into
    /// the scratch list, which is taken: give it back with
    /// [`Conquest::give_back`].
    fn walkway_stretch(&mut self, i: usize) -> Vec<usize> {
        let (s, m) = (&self.structures, &mut self.manning);
        let owner = s.owner_of(i);
        let walk =
            |j: usize| s.owner_of(j) == owner && s.walkway(j) && s.finished(j) && s.powered(j);
        s.stretch(i, walk, STAIR_TILES, &mut m.stretch, &mut m.others);
        core::mem::take(&mut m.stretch)
    }

    fn give_back(&mut self, stretch: Vec<usize>) {
        self.manning.stretch = stretch;
    }

    /// The building nearest `here` to climb wall slot `i` from: a finished
    /// tower or gate of its owner's touching the stretch of walkway it is
    /// on, or an end of that stretch; failing both (a closed ring), `i`.
    fn stair(&mut self, i: usize, here: FxVec2) -> StructureHandle {
        let stretch = self.walkway_stretch(i);
        let s = &self.structures;
        let owner = s.owner_of(i);
        let mut best: Option<(Fx, usize)> = None;
        let mut consider = |j: usize| {
            let gap = (s.centre(j) - here).length();
            if best.is_none_or(|(g, b)| (gap, j) < (g, b)) {
                best = Some((gap, j));
            }
        };
        // Towers and gates joined to the wall.
        for j in s.standing() {
            if s.owner_of(j) == owner
                && s.finished(j)
                && (s.is_tower(j) || s.is_gate(j))
                && (stretch.iter()).any(|&t| foot_reaches(s.footprint(j), s.footprint(t), 0))
            {
                consider(j);
            }
        }
        // The ends of the stretch: tiles touching at most one other of it.
        for &t in &stretch {
            let touching = (stretch.iter())
                .filter(|&&o| o != t && foot_reaches(s.footprint(o), s.footprint(t), 0))
                .count();
            if touching <= 1 {
                consider(t);
            }
        }
        let stair = s.handle(best.map_or(i, |(_, j)| j));
        self.give_back(stretch);
        stair
    }

    /// Before anyone moves this tick: a regiment on a wall that was given
    /// another order (to move, attack a building, fall back or work) comes
    /// down at its foot first, so it plans its way from there (as does a
    /// melee one sent to attack a regiment on the ground); one that broke
    /// runs from there. A regiment climbing that was given another order
    /// (an attack too) stays on the ground. Read after the orders, rather than by
    /// each order, so no kind of order (a power's, say) leaves one up.
    pub(crate) fn release_held(&mut self, regs: &mut Regiments, units: &[UnitStats], nav: &Nav) {
        self.manning
            .holds
            .retain(|o| regs.resolve(o.regiment).is_some());
        let mut k = 0;
        while k < self.manning.holds.len() {
            let (regiment, phase, foot) = {
                let o = &self.manning.holds[k];
                (o.regiment, o.phase, o.foot)
            };
            let r = regiment.index as usize;
            let ordered =
                regs.target[r].is_some() || regs.falling_back[r] > 0 || self.has_job(regs, r);
            let attack = regs.attack_target[r].and_then(|t| regs.resolve(t));
            // Up there, only shooters can strike at a regiment below.
            let melee = units[regs.unit[r] as usize].range.raw() == 0;
            let reach_down = attack.is_some_and(|j| melee && !regs.on_wall[j]);
            match phase {
                Phase::Holding if ordered || reach_down || regs.morale_state[r].broken() => {
                    self.come_down(k, foot, regs, units, nav);
                }
                Phase::Climbing(_) if ordered || attack.is_some() => {
                    self.manning.holds.remove(k);
                }
                _ => k += 1,
            }
        }
    }

    /// One tick of the regiments on the walls, after the buildings: those
    /// at the foot of their stair climb, those done climbing spread along
    /// the walkway, and those whose tiles fell come down with them.
    pub(crate) fn man_walls(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
        stances: &Stances,
    ) {
        self.manning
            .holds
            .retain(|o| regs.resolve(o.regiment).is_some());
        let mut k = 0;
        while k < self.manning.holds.len() {
            let (regiment, phase, ladder) = {
                let o = &self.manning.holds[k];
                (o.regiment, o.phase, o.ladder)
            };
            let r = regiment.index as usize;
            let work = if ladder { Work::Climb } else { Work::Man };
            match phase {
                Phase::Going => {
                    if self.structures.climbed.contains(&regiment) {
                        let hold = &mut self.manning.holds[k];
                        hold.phase =
                            Phase::Climbing(if ladder { LADDER_TICKS } else { CLIMB_TICKS });
                        hold.foot = regs.position[r];
                    } else if self
                        .structures
                        .job_of(regs, r)
                        .is_none_or(|(_, w)| w != work)
                    {
                        // Given another order, routed, or no way there.
                        self.manning.holds.remove(k);
                        continue;
                    }
                }
                Phase::Climbing(left) => {
                    if regs.engaged[r].is_some() || regs.morale_state[r].broken() {
                        // Caught at the foot: it fights where it stands.
                        self.manning.holds.remove(k);
                        continue;
                    }
                    let left = left.saturating_sub(regs.steps(r));
                    if left > 0 {
                        self.manning.holds[k].phase = Phase::Climbing(left);
                        k += 1;
                        continue;
                    }
                    let up = if ladder {
                        self.top(k, r, regs, units, stances)
                    } else {
                        self.mount(k, r, regs)
                    };
                    if !up {
                        // No room left on that wall.
                        self.manning.holds.remove(k);
                        continue;
                    }
                }
                Phase::Holding => {
                    let s = &self.structures;
                    let o = &self.manning.holds[k];
                    let held = &o.tiles[..usize::from(o.held)];
                    let fell = held
                        .iter()
                        .position(|&t| s.resolve(t).is_none_or(|j| !s.powered(j)));
                    if let Some(f) = fell {
                        self.fall(k, r, f, regs, units, nav);
                        continue;
                    }
                    // Its own side's Castle Wall under every soldier gives its
                    // shooters loopholes (see [`crate::twists`]); a wall
                    // raised under it does too.
                    // (A gatehouse joined to it doesn't take them away.)
                    let ours = |t: &StructureHandle| s.owner_of(t.index as usize) == regs.owner[r];
                    let loophole = |t: &StructureHandle| s.loopholes(t.index as usize);
                    let gate = |t: &StructureHandle| s.is_gate(t.index as usize);
                    regs.loophole[r] = !regs.climber[r]
                        && held.iter().all(|t| ours(t) && (loophole(t) || gate(t)))
                        && held.iter().any(loophole);
                    if regs.climber[r]
                        && regs.engaged[r].is_none()
                        && (self.manning.defender(o.tile, k, regs, stances)).is_none()
                    {
                        // No one holds the ladder head now: onto the wall.
                        self.take_the_wall(k, r, regs);
                    }
                }
            }
            k += 1;
        }
    }

    /// Hold `k`'s regiment, slot `r`, at the top of its ladders: it fights
    /// whoever holds the tile it climbed to from the ladder head (one tile
    /// out from the wall, facing in, a few wide), or, if no one does,
    /// takes the wall. False if the wall is gone, or full.
    fn top(
        &mut self,
        k: usize,
        r: usize,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
    ) -> bool {
        let (tile, foot) = (self.manning.holds[k].tile, self.manning.holds[k].foot);
        let s = &self.structures;
        let Some(i) = s.resolve(tile).filter(|&i| s.walkway(i) && s.powered(i)) else {
            return false;
        };
        let Some(j) = self.manning.defender(tile, k, regs, stances) else {
            return self.take_the_wall(k, r, regs);
        };
        let centre = s.centre(i);
        let out = Angle::of_vec(foot - centre);
        let foot_print = s.footprint(i).map(|v| v as i16);
        let hold = &mut self.manning.holds[k];
        hold.phase = Phase::Holding;
        hold.held = 1;
        hold.tiles[0] = tile;
        hold.at[0] = foot_print;
        regs.formation[r] = Formation::Line;
        regs.braced[r] = false;
        regs.leave_fight(r);
        let head = centre + out.to_vec();
        regs.place(
            regs.handle(r),
            head,
            out.wrapping_add(Angle::HALF),
            LADDER_FRONT,
        );
        // Up against the defender's front (on the side it climbed), where
        // the tile is beyond the end of the defender's block, at that end.
        let (them, me) = (Block::of(regs, units, j), Block::of(regs, units, r));
        let along = (head - them.centre).dot(them.right);
        let along = along.clamp(-them.half_width, them.half_width);
        let side = if (foot - them.centre).dot(them.forward) < Fx::ZERO {
            them.forward * -Fx::ONE
        } else {
            them.forward
        };
        let gap = them.half_depth + me.half_depth;
        regs.position[r] = them.centre + them.right * along + side * gap;
        regs.facing[r] = Angle::of_vec(side * -Fx::ONE);
        regs.on_wall[r] = true;
        regs.climber[r] = true;
        true
    }

    /// Hold `k`'s climbers, slot `r`, with no one holding the tile they
    /// climbed to, spread along the walkway from it as a regiment manning
    /// it would, facing in, and will come down inside the walls. False if
    /// there is no room (they stay as they were).
    fn take_the_wall(&mut self, k: usize, r: usize, regs: &mut Regiments) -> bool {
        let hold = self.manning.holds[k];
        let s = &self.structures;
        let Some(i) = s.resolve(hold.tile) else {
            return false;
        };
        // The inside: across the wall as drawn from the foot of its ladders
        // (or, climbed at an end along the wall, from where it was sent).
        let centre = s.centre(i);
        let [x0, y0, x1, y1] = s.line[i];
        let n = FxVec2::from_ints(-i32::from(y1 - y0), i32::from(x1 - x0));
        let n = if n == FxVec2::ZERO {
            hold.foot - centre
        } else {
            n
        };
        let mut side = (hold.foot - centre).dot(n);
        if side == Fx::ZERO {
            side = (hold.from - centre).dot(n);
        }
        let n = Angle::of_vec(n).to_vec() * Fx::from_ratio(3, 2);
        let o = &mut self.manning.holds[k];
        o.held = 0;
        o.foot = if side < Fx::ZERO {
            centre + n
        } else {
            centre - n
        };
        // It faces away from its ladders: in.
        o.from = hold.foot;
        if !self.mount(k, r, regs) {
            self.manning.holds[k] = hold;
            return false;
        }
        regs.climber[r] = false;
        true
    }

    /// Hold `k`'s regiment, slot `r`, done climbing, takes up to one tile a
    /// soldier of the walkway it was sent to (a gatehouse three), out from
    /// that tile along the wall, and stands on them in one rank facing out. False if every
    /// tile near there is held already, or the wall is gone.
    fn mount(&mut self, k: usize, r: usize, regs: &mut Regiments) -> bool {
        let (tile, from) = (self.manning.holds[k].tile, self.manning.holds[k].from);
        let Some(i) = (self.structures.resolve(tile)).filter(|&i| self.structures.walkway(i))
        else {
            return false;
        };
        let stretch = self.walkway_stretch(i);
        let s = &self.structures;
        let want = usize::from(regs.models[r]).clamp(1, MAX_HELD);
        let mut tiles = [tile; MAX_HELD];
        let mut at = [[0; 4]; MAX_HELD];
        // Pieces held, and soldiers they have room for in one rank.
        let (mut held, mut room) = (0, 0);
        for &j in &stretch {
            let t = s.handle(j);
            if room < want && !self.manning.taken(t) {
                // One soldier a tile along it (three on a gatehouse).
                let foot = s.footprint(j);
                tiles[held] = t;
                at[held] = foot.map(|v| v as i16);
                held += 1;
                room += piece_room(foot);
            }
        }
        self.give_back(stretch);
        if held == 0 {
            return false;
        }
        let s = &self.structures;
        // Stand at the middle of the tiles (each piece weighed by its room,
        // summed from the first so it can't overflow), facing away from
        // where it was sent from (the foot is often on the wall's own
        // line), square to the wall as drawn.
        let first = s.centre(tiles[0].index as usize);
        let mut sum = FxVec2::ZERO;
        for (t, foot) in tiles[..held].iter().zip(&at) {
            let weight = Fx::from_int(piece_room(foot.map(i32::from)) as i32);
            sum += (s.centre(t.index as usize) - first) * weight;
        }
        let n = Fx::from_int(room as i32);
        let middle = first + FxVec2::new(sum.x / n, sum.y / n);
        // Along the wall: from the first tile held to the last (the
        // furthest out along it), or as the wall was drawn, or along a
        // gatehouse held alone.
        let ([ax, ay, _, _], [bx, by, _, _]) = (at[0], at[held - 1]);
        let [x0, y0, x1, y1] = s.line[i];
        let [_, _, w, d] = s.footprint(i);
        let (dx, dy) = match ((bx - ax, by - ay), (x1 - x0, y1 - y0)) {
            ((0, 0), (0, 0)) if w > d => (1, 0),
            ((0, 0), (0, 0)) => (0, 1),
            ((0, 0), line) => line,
            (d, _) => d,
        };
        let mut out = FxVec2::from_ints(-i32::from(dy), i32::from(dx));
        if out == FxVec2::ZERO {
            out = FxVec2::from_ints(0, 1);
        }
        if (middle - from).dot(out) < Fx::ZERO {
            out = out * -Fx::ONE;
        }
        let hold = &mut self.manning.holds[k];
        hold.phase = Phase::Holding;
        hold.held = held as u8;
        hold.tiles = tiles;
        hold.at = at;
        regs.formation[r] = Formation::Line;
        regs.braced[r] = false;
        regs.leave_fight(r);
        regs.place(regs.handle(r), middle, Angle::of_vec(out), room as u16);
        regs.on_wall[r] = true;
        true
    }

    /// Hold `k`'s regiment comes down off the wall to `at` (on the ground
    /// by the wall), and the hold is gone. Its order, if it was given one,
    /// stands, and it takes back the shape it had before it climbed.
    fn come_down(
        &mut self,
        k: usize,
        at: FxVec2,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
    ) {
        let o = self.manning.holds.remove(k);
        let Some(r) = regs.resolve(o.regiment) else {
            return;
        };
        if !regs.on_wall[r] {
            return;
        }
        regs.on_wall[r] = false;
        regs.climber[r] = false;
        regs.loophole[r] = false;
        regs.formation[r] = o.formation;
        regs.set_frontage(r, o.frontage);
        let at = nav.settle(regs.owner[r], at, regs.berth(units, r));
        regs.position[r] = at;
    }

    /// Tile `f` of hold `k`'s regiment (slot `r`) fell, or lost the power
    /// that held it up: the regiment comes down into the breach, every
    /// soldier who stood on that tile hurt by the fall, and all of it in
    /// disorder.
    fn fall(
        &mut self,
        k: usize,
        r: usize,
        f: usize,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
    ) {
        let o = self.manning.holds[k];
        let held = usize::from(o.held);
        // The soldiers stand spread over the pieces by their room.
        let room = |n: usize| piece_room(o.at[n].map(i32::from));
        let before: usize = (0..f).map(room).sum();
        let total: usize = (0..held).map(room).sum();
        let models = usize::from(regs.models[r]);
        let on_it = models * (before + room(f)) / total - models * before / total;
        let breach = match self.structures.resolve(o.tiles[f]) {
            // Fallen: its ground is free now.
            None => footprint_centre(o.at[f].map(i32::from)),
            // Without power the wall stands open: down at the foot.
            Some(_) => o.foot,
        };
        self.come_down(k, breach, regs, units, nav);
        regs.wound_last(r, on_it, FALL_HP);
        regs.disorder(r);
        if regs.models[r] == 0 {
            regs.despawn(o.regiment);
        }
    }
}

/// How many soldiers stand on a piece of walkway with footprint `foot` in
/// one rank: one a tile along it (three on a gatehouse).
fn piece_room(foot: [i32; 4]) -> usize {
    foot[2].max(foot[3]).max(1) as usize
}

impl crate::structures::Structures {
    /// Whether building slot `i` is a drawn wall or a gate with a walkway
    /// to stand on: a level above the first (one that upgrades from
    /// another), a gatehouse for a gate.
    pub(crate) fn walkway(&self, i: usize) -> bool {
        (self.is_line(i) || self.is_gate(i)) && self.stats_of(i).upgrades_from.is_some()
    }
}
