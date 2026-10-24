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

use crate::conquest::{Conquest, overgrown, overlaps};
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{BuildingId, GATE, LINE, Refusal, StructureHandle, Structures, Work};
use crate::terrain::Terrain;
use crate::{PlayerId, Vec};

/// The most tiles a wall drawn in one order covers.
pub const MAX_WALL_TILES: usize = 32;

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
        self.standing()
            .filter(|&i| self.owner_of(i) == owner && self.flags_of(i) & (LINE | GATE) != 0)
            .map(|i| self.footprint(i))
            .filter(|&foot| overlaps(foot, area))
            .collect()
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
        for t in plan {
            let i = self.take_slot(owner, building, t.tile, 0).expect("room");
            self.lay_foundation(i);
            self.line[i] = line;
            let h = self.handle(i);
            ends = Some(ends.map_or((h, h), |(first, _)| (first, h)));
        }
        let feet: Vec<[i32; 4]> = plan.iter().map(|t| [t.tile.0, t.tile.1, 1, 1]).collect();
        nav.set_built_all(&feet, true, regs);
        ends
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
        let b = *self
            .structures
            .buildings()
            .get(usize::from(building))
            .ok_or(Refusal::Unknown)?;
        if b.flags & LINE == 0 {
            return Err(Refusal::Unknown);
        }
        let tiles = wall_tiles(from, to).ok_or(Refusal::Unknown)?;
        self.may_lay(player, building)?;
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
            .map(|tile| {
                let fits = (self.check(building, tile, 0, nav, regs, terrain))
                    .and_then(|()| self.powered(player, building, tile, 0))
                    .and_then(|()| {
                        if overgrown(terrain, [tile.0, tile.1, 1, 1]) {
                            Err(Refusal::Overgrown)
                        } else {
                            Ok(())
                        }
                    });
                WallTile { tile, fits }
            })
            .collect();
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

    /// Worker slot `r`, `player`'s, lays a wall of type `building` from tile
    /// `from` to tile `to` (see [`Conquest::wall_plan`]), pays for all of
    /// it and goes to build the tile nearest it. Refused if any tile can't
    /// go up, or there is nothing new to build.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lay_wall(
        &mut self,
        player: PlayerId,
        r: usize,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &mut Nav,
        regs: &mut Regiments,
        terrain: &Terrain,
    ) -> bool {
        // A worker used up on the building it starts would leave the rest
        // of the wall unbuilt.
        let kind = self.unit(regs.unit[r]);
        if !kind.worker || kind.consumed {
            return false;
        }
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
        let stats = self.structures.buildings()[usize::from(building)];
        let n = plan.len() as u32;
        self.economy.spend(
            player,
            u32::from(stats.cost_grub) * n,
            u32::from(stats.cost_marrow) * n,
        );
        // It starts at the end nearer it and works along.
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
