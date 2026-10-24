//! Manning the walls (docs/26-fortifications.md, "Manning the walls"): a
//! regiment sent onto its side's wall of level 2 or 3 walks to the nearest
//! stair (a tower or gate joined to the wall, or an end of it), climbs,
//! and spreads along the walkway in one rank, facing out.
//!
//! Up there it is off the walking grid ([`crate::regiments`] `on_wall`):
//! the wall holds it where it stands, melee from the ground can't reach
//! it (nor it the ground), its shooters reach [`WALL_RANGE_PERCENT`] as
//! far and arrows at it do [`WALL_COVER_PERCENT`] of their damage. Any
//! other order brings it down at the foot it climbed from. When a tile it
//! holds falls (or a Choir wall under it loses power) the whole regiment
//! comes down with it into the breach, hurt and in disorder.

use crate::Formation;
use crate::conquest::Conquest;
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::{Refusal, StructureHandle, Work, footprint_centre};
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
/// Hit points each soldier on a tile that falls loses.
pub const FALL_HP: u32 = 10;
/// How far along a wall a regiment looks for its stair, in tiles.
const STAIR_TILES: usize = 128;

/// Where a regiment sent onto a wall is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Walking to the stair (its [`Work::Man`] job).
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
    /// The tiles it holds, the first `held` of them, and where each
    /// stands (to come down where one fell).
    held: u8,
    tiles: [StructureHandle; MAX_HELD],
    at: [(i16, i16); MAX_HELD],
}

/// The regiments on the walls (see the module docs).
#[derive(Clone, Debug, Default)]
pub(crate) struct Manning {
    holds: Vec<Hold>,
    /// Scratch for [`Conquest::walkway_stretch`], kept so a tick never
    /// allocates.
    stretch: Vec<usize>,
    others: Vec<(i32, i32, usize)>,
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
        self.holds
            .iter()
            .any(|h| h.tiles[..usize::from(h.held)].contains(&t))
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
            for &(x, y) in &o.at[..usize::from(o.held)] {
                h.u16(x as u16);
                h.u16(y as u16);
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
        let s = &self.structures;
        let i = s.owned(h, player).ok_or(Refusal::Unknown)?;
        if !s.is_line(i) || !self.unit(regs.unit[r]).mans_walls {
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
        let Ok(i) = self.man_check(player, r, regs, h) else {
            return false;
        };
        if let Some(k) = self.manning.find(r) {
            let foot = self.manning.holds[k].foot;
            self.come_down(k, foot, regs, units, nav);
        }
        self.stop_work(r);
        let stair = self.stair(i, regs.position[r]);
        self.structures.send(regs, r, stair, Work::Man);
        self.manning.holds.push(Hold {
            regiment: regs.handle(r),
            phase: Phase::Going,
            tile: h,
            foot: regs.position[r],
            from: regs.position[r],
            formation: regs.formation[r],
            frontage: regs.frontage[r],
            held: 0,
            tiles: [h; MAX_HELD],
            at: [(0, 0); MAX_HELD],
        });
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
    pub(crate) fn man_walls(&mut self, regs: &mut Regiments, units: &[UnitStats], nav: &Nav) {
        self.manning
            .holds
            .retain(|o| regs.resolve(o.regiment).is_some());
        let mut k = 0;
        while k < self.manning.holds.len() {
            let (regiment, phase) = {
                let o = &self.manning.holds[k];
                (o.regiment, o.phase)
            };
            let r = regiment.index as usize;
            match phase {
                Phase::Going => {
                    if self.structures.climbed.contains(&regiment) {
                        let hold = &mut self.manning.holds[k];
                        hold.phase = Phase::Climbing(CLIMB_TICKS);
                        hold.foot = regs.position[r];
                    } else if !matches!(self.structures.job_of(regs, r), Some((_, Work::Man))) {
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
                    } else if !self.mount(k, r, regs) {
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
                }
            }
            k += 1;
        }
    }

    /// Hold `k`'s regiment, slot `r`, done climbing, takes up to one tile a
    /// soldier of the walkway it was sent to, out from that tile along the
    /// wall, and stands on them in one rank facing out. False if every
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
        let mut at = [(0, 0); MAX_HELD];
        let mut held = 0;
        for &j in &stretch {
            let t = s.handle(j);
            if held < want && !self.manning.taken(t) {
                let [x, y, _, _] = s.footprint(j);
                tiles[held] = t;
                at[held] = (x as i16, y as i16);
                held += 1;
            }
        }
        self.give_back(stretch);
        if held == 0 {
            return false;
        }
        let s = &self.structures;
        // Stand at the middle of the tiles, facing away from where it was
        // sent from (the foot is often on the wall's own line), square to
        // the wall as drawn.
        let mut sum = FxVec2::ZERO;
        for &t in &tiles[..held] {
            sum += s.centre(t.index as usize);
        }
        let n = Fx::from_int(held as i32);
        let middle = FxVec2::new(sum.x / n, sum.y / n);
        // Along the wall: from the first tile held to the last (the
        // furthest out along it), or as the wall was drawn.
        let ((ax, ay), (bx, by)) = (at[0], at[held - 1]);
        let [x0, y0, x1, y1] = s.line[i];
        let (dx, dy) = match (bx - ax, by - ay) {
            (0, 0) => (x1 - x0, y1 - y0),
            d => d,
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
        regs.place(regs.handle(r), middle, Angle::of_vec(out), held as u16);
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
        // Soldier `m` stands on tile `m % held`.
        let models = usize::from(regs.models[r]);
        let on_it = models / held + usize::from(f < models % held);
        let breach = match self.structures.resolve(o.tiles[f]) {
            // Fallen: its ground is free now.
            None => {
                let (x, y) = o.at[f];
                footprint_centre([i32::from(x), i32::from(y), 1, 1])
            }
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

impl crate::structures::Structures {
    /// Whether building slot `i` is a drawn wall with a walkway to stand
    /// on: a level above the first (one that upgrades from another).
    pub(crate) fn walkway(&self, i: usize) -> bool {
        self.is_line(i) && self.stats_of(i).upgrades_from.is_some()
    }
}
