//! Drawn walls (docs/26-fortifications.md, "Drawn walls").
//!
//! A wall building ([`LINE`]) is drawn as a straight line from one tile to
//! another at any angle, [`Command::BuildWall`](crate::Command::BuildWall).
//! Each tile of the line is a building of its own, with its own hit points,
//! so a breach is local, and the line is a staircase of tiles in the
//! walking grid. Two tiles that only touch at a corner still seal the way:
//! a step across a corner needs both cells beside it open. Its stats are
//! for one tile, so its price, hit points and build time are per tile.
//!
//! Tiles where one of the player's own walls or gates already stands are
//! left out, so lines join and cross. The whole line is refused if any of
//! the rest can't be built (see [`Conquest::wall_plan`]); the worker that
//! lays it builds it tile by tile from the end nearer it, going on each
//! time to the next unfinished tile of its side's walls beside it (see
//! [`Structures::next_wall_tile`]).
//!
//! A Brood wall ([`GROWN`]) is grown, not built: it takes no worker, goes
//! only where its owner's root-veins reach, and every tile of it grows at
//! once on its own. A tile its veins no longer reach withers
//! ([`WITHER_PER_SECOND`]) until it dies.

use crate::conquest::{Conquest, overgrown, overlaps};
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{
    BuildingId, BuildingStats, GATE, GROWN, LINE, Refusal, StructureHandle, Structures, Work,
};
use crate::terrain::Terrain;
use crate::{PlayerId, Vec};

/// The most tiles a wall drawn in one order covers.
pub const MAX_WALL_TILES: usize = 32;

/// Hit points a second a Brood wall tile off its owner's veins loses.
pub const WITHER_PER_SECOND: u32 = 10;

/// How near, in tiles, a worker that finishes a tile of wall looks for the
/// next to build.
const CARRY_ON: i32 = 2;

/// The tiles of a wall drawn from tile `from` to tile `to`, both included,
/// in order: a line from centre to centre, a tile wide (8-connected, so a
/// diagonal is a staircase of tiles meeting at corners). `None` when it
/// would cover more than [`MAX_WALL_TILES`].
pub fn wall_tiles(from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let steps = dx.abs().max(dy.abs());
    if steps as usize >= MAX_WALL_TILES {
        return None;
    }
    // Rounded to the nearest tile, halves away from the start, so the same
    // two ends always give the same tiles.
    let at = |k: i32, d: i32| {
        if steps == 0 {
            return 0;
        }
        let n = 2 * k * d;
        let q = (n.abs() + steps) / (2 * steps);
        q * n.signum()
    };
    Some(
        (0..=steps)
            .map(|k| (from.0 + at(k, dx), from.1 + at(k, dy)))
            .collect(),
    )
}

/// One tile of a planned wall: where, and whether it can go up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallTile {
    pub tile: (i32, i32),
    pub fits: Result<(), Refusal>,
}

impl Structures {
    /// The footprints of `owner`'s own walls and gates that touch the box
    /// `area`.
    fn own_walls_in(&self, owner: PlayerId, area: [i32; 4]) -> Vec<[i32; 4]> {
        self.own_in(owner, LINE | GATE, area)
            .map(|i| self.footprint(i))
            .collect()
    }

    /// The slots of `owner`'s own buildings with any of `flags` that touch
    /// the box `area`.
    pub(crate) fn own_in(
        &self,
        owner: PlayerId,
        flags: u16,
        area: [i32; 4],
    ) -> impl Iterator<Item = usize> + '_ {
        self.standing().filter(move |&i| {
            self.owner_of(i) == owner
                && self.flags_of(i) & flags != 0
                && overlaps(self.footprint(i), area)
        })
    }

    /// Building slot `i` is a tile of a drawn wall.
    pub(crate) fn is_line(&self, i: usize) -> bool {
        self.flags_of(i) & LINE != 0
    }

    /// The next tile of `owner`'s walls to build after slot `i`: the
    /// nearest unfinished one within [`CARRY_ON`] tiles of it, those drawn
    /// in the same line first, then the lowest slot; failing that the
    /// nearest within [`MAX_WALL_TILES`], so a builder at the end of one
    /// line goes back for the rest of the wall.
    pub(crate) fn next_wall_tile(&self, owner: PlayerId, i: usize) -> Option<usize> {
        let [x, y, _, _] = self.footprint(i);
        self.standing()
            .filter(|&j| {
                j != i && self.owner_of(j) == owner && self.is_line(j) && !self.finished(j)
            })
            .map(|j| {
                let [jx, jy, _, _] = self.footprint(j);
                let gap = (jx - x).abs().max((jy - y).abs());
                let far = gap > CARRY_ON;
                (far, !far && self.line[j] != self.line[i], gap, j)
            })
            .filter(|&(_, _, gap, _)| gap <= MAX_WALL_TILES as i32)
            .min()
            .map(|(.., j)| j)
    }

    /// Lay foundations for `owner`'s wall building `building` on each tile
    /// of `plan`, the wall drawn from `line`'s first tile to its second, and
    /// stamp them into the walking grid at once. The first and last tiles
    /// laid, or `None`, with nothing laid, when the plan is empty or the
    /// pool hasn't room; the caller has checked each tile.
    pub(crate) fn found_wall(
        &mut self,
        owner: PlayerId,
        building: BuildingId,
        plan: &[WallTile],
        line: [i16; 4],
        nav: &mut Nav,
        regs: &Regiments,
    ) -> Option<(StructureHandle, StructureHandle)> {
        if plan.is_empty() || self.free_slots() < plan.len() {
            return None;
        }
        let mut ends = None;
        let grown = self.buildings()[usize::from(building)].flags & GROWN != 0;
        for t in plan {
            let i = self.take_slot(owner, building, t.tile, 0).expect("room");
            self.lay_foundation(i);
            self.line[i] = line;
            self.growing[i] = grown;
            let h = self.handle(i);
            ends = Some(ends.map_or((h, h), |(first, _)| (first, h)));
        }
        let feet: Vec<[i32; 4]> = plan.iter().map(|t| [t.tile.0, t.tile.1, 1, 1]).collect();
        nav.set_built_all(&feet, true, regs);
        ends
    }

    /// One tick: each tile of a Brood wall its owner's veins don't reach
    /// withers, and falls when it has no hit points left. The tiles of a
    /// cut-off wall fall together, so they come down in one change to the
    /// walking grid (the list is made only when some fall).
    pub(crate) fn wither(&mut self, nav: &mut Nav, regs: &Regiments) {
        let mut falling = Vec::new();
        for i in 0..self.line.len() {
            if !self.stands(i) || self.flags_of(i) & GROWN == 0 || self.rooted(i) {
                continue;
            }
            if self.wear(i, WITHER_PER_SECOND) {
                falling.push(self.handle(i));
            }
        }
        if !falling.is_empty() {
            self.fallen.extend_from_slice(&falling);
            self.despawn_all(&falling, nav, regs);
        }
    }
}

impl Conquest {
    /// The tiles of a wall of type `building` `player` would draw from tile
    /// `from` to tile `to`, leaving out those its own walls and gates
    /// already stand on, each with whether it can go up there. Refused as a
    /// whole for a building that isn't a drawn wall, a line too long, or
    /// one its owner may not lay now (tier, or the price of all of it).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn wall_plan(
        &self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<Vec<WallTile>, Refusal> {
        let (b, plan) = self.wall_fits(player, building, from, to, nav, regs, terrain)?;
        if self.structures.free_slots() < plan.len() {
            return Err(Refusal::NoRoom);
        }
        let n = plan.len() as u32;
        if !self.economy.can_afford(
            player,
            u32::from(b.cost_grub) * n,
            u32::from(b.cost_marrow) * n,
        ) {
            return Err(Refusal::Price);
        }
        Ok(plan)
    }

    /// [`Conquest::wall_plan`] as a blueprint (see [`crate::blueprints`]):
    /// the tiles not yet walled or planned, each with whether it could go up
    /// now, the price and room left out. Refused as well for a grown wall,
    /// which needs no worker to plan for.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn blueprint(
        &self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<Vec<WallTile>, Refusal> {
        let (b, mut plan) = self.wall_fits(player, building, from, to, nav, regs, terrain)?;
        if b.flags & GROWN != 0 {
            return Err(Refusal::Unknown);
        }
        plan.retain(|t| !self.blueprints.planned(player, t.tile));
        if self.blueprints.room(player) < plan.len() {
            return Err(Refusal::NoRoom);
        }
        Ok(plan)
    }

    /// [`Conquest::wall_plan`] without the price or room: the tiles not
    /// yet walled, each with whether it could go up now, for a wall
    /// `player` may lay at all.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn wall_fits(
        &self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<(BuildingStats, Vec<WallTile>), Refusal> {
        self.may_build(player, building)?;
        self.wall_line(player, building, from, to, nav, regs, terrain)
    }

    /// The tiles of a wall of type `building` drawn from `from` to `to`
    /// that `player`'s own walls and gates don't already stand on, each with
    /// whether it could go up now.
    #[allow(clippy::too_many_arguments)]
    fn wall_line(
        &self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<(BuildingStats, Vec<WallTile>), Refusal> {
        let b = *self
            .structures
            .buildings()
            .get(usize::from(building))
            .ok_or(Refusal::Unknown)?;
        if b.flags & LINE == 0 {
            return Err(Refusal::Unknown);
        }
        let tiles = wall_tiles(from, to).ok_or(Refusal::Unknown)?;
        let area = [
            from.0.min(to.0),
            from.1.min(to.1),
            (to.0 - from.0).abs() + 1,
            (to.1 - from.1).abs() + 1,
        ];
        let own = self.structures.own_walls_in(player, area);
        let plan: Vec<WallTile> = tiles
            .into_iter()
            .filter(|&t| !own.iter().any(|&foot| overlaps(foot, [t.0, t.1, 1, 1])))
            .map(|tile| WallTile {
                tile,
                fits: self.wall_tile_fits(player, building, tile, nav, regs, terrain),
            })
            .collect();
        Ok((b, plan))
    }

    /// Whether a tile of wall `building` can go up at `tile` for `player`
    /// now, going by the ground alone.
    pub(crate) fn wall_tile_fits(
        &self,
        player: PlayerId,
        building: BuildingId,
        tile: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Result<(), Refusal> {
        self.check(&[], building, tile, 0, nav, regs, terrain)?;
        self.powered(player, building, tile, 0)?;
        self.rooted(player, building, tile, 0)?;
        if overgrown(terrain, [tile.0, tile.1, 1, 1]) {
            return Err(Refusal::Overgrown);
        }
        Ok(())
    }

    /// Worker slot `r`, `player`'s, lays a wall of type `building` from tile
    /// `from` to tile `to` (see [`Conquest::wall_plan`]), pays for all of
    /// it and goes to build the tile nearest it; a wall grown along veins
    /// takes no worker (`r` is `None`) and grows on its own. Refused if
    /// any tile can't go up, or there is nothing new to build.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lay_wall(
        &mut self,
        player: PlayerId,
        r: Option<usize>,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &mut Nav,
        regs: &mut Regiments,
        terrain: &Terrain,
    ) -> bool {
        let Some(&stats) = self.structures.buildings().get(usize::from(building)) else {
            return false;
        };
        // A grown wall takes no worker; any other a worker that isn't used
        // up on the building it starts, which would leave the rest unbuilt.
        let worker = match (r, stats.flags & GROWN != 0) {
            (None, true) => None,
            (Some(r), false) => {
                let kind = self.unit(regs.unit[r]);
                if !kind.worker || kind.consumed {
                    return false;
                }
                Some(r)
            }
            _ => return false,
        };
        let Ok(plan) = self.wall_plan(player, building, from, to, nav, regs, terrain) else {
            return false;
        };
        if plan.iter().any(|t| t.fits.is_err()) {
            return false;
        }
        let line = [from.0, from.1, to.0, to.1].map(|v| v as i16);
        let Some((a, b)) = self
            .structures
            .found_wall(player, building, &plan, line, nav, regs)
        else {
            return false;
        };
        let n = plan.len() as u32;
        self.economy.spend(
            player,
            u32::from(stats.cost_grub) * n,
            u32::from(stats.cost_marrow) * n,
        );
        // Tiles drawn over the player's own blueprint are no longer planned.
        if !self.blueprints.plans().is_empty() {
            let tiles: Vec<(i32, i32)> = plan.iter().map(|t| t.tile).collect();
            self.blueprints.drop_tiles(player, &tiles);
        }
        let Some(r) = worker else {
            // Grown: every tile grows on its own.
            return true;
        };
        // The worker starts at the end nearer it and works along.
        let here = regs.position[r];
        let gap = |h: &StructureHandle| {
            let c = self.structures.centre(h.index as usize) - here;
            c.x.abs().max(c.y.abs())
        };
        let first = if gap(&b) < gap(&a) { b } else { a };
        self.stop_work(r);
        self.structures.send(regs, r, first, Work::Build);
        true
    }
}
