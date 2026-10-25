//! Field works (docs/26-fortifications.md, "Field works"): stakes and
//! barricades, drawn a tile at a time like walls.
//!
//! - **Stakes** ([`Role::Stakes`]) stand on nothing on the walkable grid:
//!   anyone walks through a stake tile, at [`STAKE_PACE_PERCENT`] of their
//!   pace, and nobody charges into or out of one. A heavy regiment (riders,
//!   monsters, engines) running onto an enemy's at a charge is stopped dead: each
//!   soldier of its front rank loses [`STAKE_HP`], it falls into disorder,
//!   and the stakes there lose [`STAKE_WEAR`] of their own.
//! - **Plant stakes** ([`crate::Command::PlantStakes`]): spearmen or
//!   archers standing still for [`PLANT_TICKS`] hammer a free stake line
//!   across their front, a tile in front of it.
//! - **Barricades** ([`Role::Barricade`]) are built on the grid like a
//!   wall: nobody walks through. A regiment within [`COVER_TILES`] of its
//!   own side's finished barricade takes [`COVER_ARROW_PERCENT`] from
//!   arrows and [`COVER_MELEE_PERCENT`] from blows to its front.

use alloc::vec::Vec;

use crate::combat::RUN_UP_TICKS;
use crate::conquest::Conquest;
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::{HP_SCALE, Refusal, Role, Structures};
use crate::terrain::Terrain;
use crate::{BuildingId, Mass, PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};
use gc_fixed::{Fx, FxVec2};

/// Pace kept on a stake tile, percent.
pub const STAKE_PACE_PERCENT: i32 = 50;
/// What a heavy regiment's front-rank soldiers each lose running onto
/// stakes at a charge.
pub const STAKE_HP: u32 = 12;
/// What a stake tile loses to each regiment it stops.
pub const STAKE_WEAR: u32 = 40;
/// How long a regiment stands to plant its stakes.
pub const PLANT_TICKS: u16 = 10 * TICKS_PER_SECOND as u16;
/// How far a regiment's middle may be from its own side's barricade and
/// still be covered by it, in tiles (edge to edge).
pub const COVER_TILES: i32 = 2;
/// Arrow damage taken behind a barricade, percent.
pub const COVER_ARROW_PERCENT: u64 = 70;
/// Melee damage taken behind a barricade, percent.
pub const COVER_MELEE_PERCENT: u64 = 80;

/// A field work's tile finished (`on`) or gone, for the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FieldMark {
    pub tile: (i32, i32),
    pub barricade: bool,
    pub owner: PlayerId,
    pub on: bool,
}

/// A regiment planting stakes: how long it has stood, and the stake type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Planting {
    pub regiment: RegimentHandle,
    pub building: BuildingId,
    pub left: u16,
    /// Where it stood when it began: a step off it calls the work off.
    pub at: FxVec2,
}

impl Structures {
    /// Whether building slot `i` stands on the walkable grid as built: all
    /// but stakes, which are walked through, and a finished bridge, which
    /// is walked over (see [`crate::bridges`]).
    pub(crate) fn walls_ground(&self, i: usize) -> bool {
        let stats = self.stats_of(i);
        if stats.walked_through() {
            false
        } else if stats.is_bridge() {
            !self.finished(i)
        } else {
            true
        }
    }

    /// Building slot `i` is finished or put up whole: a bridge opens its
    /// deck, a field work starts working on the ground.
    pub(crate) fn ground_up(&mut self, i: usize) {
        self.ground_mark(i, true);
    }

    /// Building slot `i` is coming down: if it was finished, the ground
    /// forgets it.
    pub(crate) fn ground_down(&mut self, i: usize) {
        if self.finished(i) {
            self.ground_mark(i, false);
        }
    }

    fn ground_mark(&mut self, i: usize, on: bool) {
        let foot = self.footprint(i);
        let barricade = match self.stats_of(i).role {
            Role::Bridge => {
                self.decks.push((foot, on));
                return;
            }
            Role::Stakes => false,
            Role::Barricade => true,
            _ => return,
        };
        self.field.push(FieldMark {
            tile: (foot[0], foot[1]),
            barricade,
            owner: self.owner_of(i),
            on,
        });
    }
}

/// Bring the ground up to date with the field works finished and gone
/// since last time (`changes`, drained).
pub(crate) fn settle_field(changes: &mut Vec<FieldMark>, terrain: &mut Terrain) {
    for m in changes.drain(..) {
        let (x, y) = m.tile;
        if m.barricade {
            terrain.set_cover(x, y, m.owner, m.on);
        } else {
            terrain.set_stakes(x, y, m.on);
        }
    }
}

/// One tick of stakes: each heavy regiment whose front runs onto an enemy's
/// stake tile at a charge (a run-up, at three quarters of its speed or
/// more) is stopped by it, hurt, and thrown into disorder, and the stakes
/// there are worn down (and may fall).
pub(crate) fn stakes_bite(
    structures: &mut Structures,
    regs: &mut Regiments,
    units: &[UnitStats],
    terrain: &Terrain,
    nav: &mut Nav,
) {
    if !terrain.any_stakes() {
        return;
    }
    for i in 0..regs.alive.len() {
        if !regs.alive[i]
            || regs.on_wall[i]
            || regs.run_up[i] < RUN_UP_TICKS
            || regs.pace[i].raw() * 4 < regs.speed[i].raw() * 3
        {
            continue;
        }
        let unit = &units[usize::from(regs.unit[i])];
        if unit.mass != Mass::Heavy {
            continue;
        }
        // Its front rank runs onto them, while the rest still has the
        // ground for a charge.
        let (_, depth) = unit.block_size_in(regs.formation[i], regs.files[i], regs.models[i]);
        let p = regs.position[i] + regs.facing[i].to_vec() * (depth / 2);
        if !terrain.staked(p) {
            continue;
        }
        let tile = (p.x.floor_int(), p.y.floor_int());
        let Some(s) = structures.stakes_at(tile) else {
            continue;
        };
        if structures.owner_of(s) == regs.owner[i] {
            continue;
        }
        let front = u32::from(regs.files[i].min(regs.models[i]));
        // Stopped dead, as on braced points.
        regs.run_up[i] = 0;
        regs.charge[i] = 0;
        regs.disorder(i);
        regs.halt(i);
        regs.take_damage(i, STAKE_HP * front);
        if regs.models[i] == 0 {
            regs.despawn(regs.handle(i));
        }
        structures.wear_down(s, STAKE_WEAR * HP_SCALE, nav, regs);
    }
}

impl Structures {
    /// Whether any stake tile, finished or not, stands in `foot`.
    pub(crate) fn stakes_in(&self, foot: [i32; 4]) -> bool {
        let [x, y, w, d] = foot;
        let lo = self.stake_tiles.partition_point(|&(t, _)| t < (x, y));
        self.stake_tiles[lo..]
            .iter()
            .take_while(|&&(t, _)| t.0 < x + w)
            .any(|&((_, ty), _)| ty >= y && ty < y + d)
    }

    /// The finished stake tile standing at `tile`, if any.
    fn stakes_at(&self, tile: (i32, i32)) -> Option<usize> {
        let at = self.stake_tiles.partition_point(|&(t, _)| t < tile);
        let &(t, s) = self.stake_tiles.get(at)?;
        (t == tile && self.finished(s as usize)).then_some(s as usize)
    }
}

/// Where regiment slot `i`'s stake line goes: the tiles a tile in front of
/// its front rank, across its frontage.
pub(crate) fn stake_line(regs: &Regiments, units: &[UnitStats], i: usize) -> Vec<(i32, i32)> {
    let unit = &units[usize::from(regs.unit[i])];
    let (width, depth) = unit.block_size_in(regs.formation[i], regs.files[i], regs.models[i]);
    let ahead = regs.facing[i].to_vec();
    let across = FxVec2::new(-ahead.y, ahead.x) * (width / 2);
    let front = regs.position[i] + ahead * (depth / 2 + Fx::ONE);
    let tile = |p: FxVec2| (p.x.floor_int(), p.y.floor_int());
    crate::walls::wall_tiles(tile(front - across), tile(front + across)).unwrap_or_default()
}

/// How far a planting regiment may drift from where it began, in raw
/// fixed point squared (a quarter tile).
const PLANT_DRIFT_SQ: u64 = (Fx::ONE.raw() as u64 / 4) * (Fx::ONE.raw() as u64 / 4);

impl Conquest {
    /// Whether regiment slot `i` may plant stakes of type `building` (see
    /// [`crate::Command::PlantStakes`]), or why not: spearmen or archers,
    /// not fighting, on a wall or broken, and a stake type.
    pub(crate) fn plant_check(
        &self,
        i: usize,
        regs: &Regiments,
        units: &[UnitStats],
        building: BuildingId,
    ) -> Result<(), Refusal> {
        let b = self
            .structures
            .buildings()
            .get(usize::from(building))
            .ok_or(Refusal::Unknown)?;
        let unit = units
            .get(usize::from(regs.unit[i]))
            .ok_or(Refusal::Unknown)?;
        if b.role != Role::Stakes || !(unit.reach || unit.range > Fx::ZERO) {
            return Err(Refusal::Unknown);
        }
        if regs.engaged[i].is_some() || regs.on_wall[i] || regs.morale_state[i].broken() {
            return Err(Refusal::Busy);
        }
        Ok(())
    }

    /// Regiment slot `i` halts to plant a line of stakes of type
    /// `building` across its front (see [`PLANT_TICKS`]).
    pub(crate) fn plant_stakes(
        &mut self,
        i: usize,
        regs: &mut Regiments,
        units: &[UnitStats],
        building: BuildingId,
    ) -> bool {
        if self.plant_check(i, regs, units, building).is_err() {
            return false;
        }
        let regiment = regs.handle(i);
        self.planting.retain(|p| p.regiment != regiment);
        regs.halt(i);
        self.planting.push(Planting {
            regiment,
            building,
            left: PLANT_TICKS,
            at: regs.position[i],
        });
        true
    }

    /// One tick of planting: a regiment that has stood long enough puts
    /// up its stake line, free, where the ground takes it; one that moved,
    /// fought or broke stops.
    pub(crate) fn tend_planting(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &mut Nav,
        terrain: &Terrain,
    ) {
        if self.planting.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.planting.retain_mut(|p| {
            let Some(i) = regs.resolve(p.regiment) else {
                return false;
            };
            let drift = (regs.position[i] - p.at).length_sq_raw();
            if drift > PLANT_DRIFT_SQ || regs.engaged[i].is_some() || regs.morale_state[i].broken()
            {
                return false;
            }
            p.left -= 1;
            if p.left == 0 {
                done.push((i, p.building));
                return false;
            }
            true
        });
        for (i, building) in done {
            let owner = regs.owner[i];
            for tile in stake_line(regs, units, i) {
                if self
                    .wall_tile_fits(owner, building, tile, nav, regs, terrain)
                    .is_ok()
                {
                    self.structures.spawn(owner, building, tile, 0, nav, regs);
                }
            }
        }
    }

    pub(crate) fn hash_planting(&self, regs: &Regiments, h: &mut StateHasher) {
        for p in &self.planting {
            if regs.resolve(p.regiment).is_none() {
                continue;
            }
            h.u32(p.regiment.index);
            h.u16(p.regiment.generation);
            h.u16(p.building);
            h.u16(p.left);
            h.i32(p.at.x.raw());
            h.i32(p.at.y.raw());
        }
    }
}
