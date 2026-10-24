//! Blueprints (docs/26-fortifications.md, "Blueprints"): a wall planned
//! ahead and paid for tile by tile as workers reach it, so a long wall can
//! be laid out early and rise as Grub comes in.
//!
//! A planned tile ([`Plan`]) is only a mark: it holds no ground, costs
//! nothing and stops nobody. Workers set to the plans
//! ([`crate::Command::WorkPlan`]) each take the nearest planned tile of
//! their side's whenever they have nothing else to do, pay for it, lay it
//! and build it, then go on to the next; when their side can't pay yet
//! they wait. A planned tile whose ground has been taken meanwhile (a
//! building, a regiment standing there, lost power) is dropped.

use crate::conquest::Conquest;
use crate::nav::Nav;
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::Refusal;
use crate::structures::{BuildingId, Work};
use crate::terrain::Terrain;
use crate::walls::WallTile;
use crate::{PlayerId, StateHasher, Vec};

/// The most planned tiles a side holds at once.
pub const MAX_PLANS: usize = 256;

/// How many of the nearest planned tiles a worker tries in a tick when
/// the nearest are crowded.
const TRIES: usize = 4;

/// One planned tile of wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub owner: PlayerId,
    pub building: BuildingId,
    pub tile: (i32, i32),
    /// The line it was planned in: its two end tiles.
    pub line: [i16; 4],
}

#[derive(Default)]
pub(crate) struct Blueprints {
    /// In the order they were planned.
    plans: Vec<Plan>,
    /// Workers set to work the plans.
    workers: Vec<RegimentHandle>,
}

impl Blueprints {
    pub(crate) fn plans(&self) -> &[Plan] {
        &self.plans
    }

    /// Whether `owner` has a tile planned at `tile`.
    pub(crate) fn planned(&self, owner: PlayerId, tile: (i32, i32)) -> bool {
        self.plans
            .iter()
            .any(|p| p.owner == owner && p.tile == tile)
    }

    /// Room for this many more planned tiles of `owner`'s.
    pub(crate) fn room(&self, owner: PlayerId) -> usize {
        MAX_PLANS.saturating_sub(self.plans.iter().filter(|p| p.owner == owner).count())
    }

    /// The parties in the mask `out` left the field: their plans go.
    pub(crate) fn drop_sides(&mut self, out: u8) {
        self.plans.retain(|p| out & (1 << p.owner) == 0);
    }

    pub(crate) fn add(&mut self, plan: Plan) {
        self.plans.push(plan);
    }

    /// Drop `owner`'s plans on any of `tiles`; whether there were any.
    pub(crate) fn drop_tiles(&mut self, owner: PlayerId, tiles: &[(i32, i32)]) -> bool {
        self.drop_where(|p| p.owner == owner && tiles.contains(&p.tile))
    }

    /// Drop the plans `gone` picks; whether there were any.
    fn drop_where(&mut self, gone: impl Fn(&Plan) -> bool) -> bool {
        let before = self.plans.len();
        self.plans.retain(|p| !gone(p));
        self.plans.len() < before
    }

    /// Drop plan `k` (an index into [`Blueprints::plans`]).
    pub(crate) fn take(&mut self, k: usize) -> Plan {
        self.plans.remove(k)
    }

    /// The plan of `owner`'s nearest to `at` (in tiles, the larger of the
    /// two distances; the earliest planned on a tie), as an index, leaving
    /// out those in `skip`.
    pub(crate) fn nearest(&self, owner: PlayerId, at: (i32, i32), skip: &[usize]) -> Option<usize> {
        (self.plans.iter().enumerate())
            .filter(|&(k, p)| p.owner == owner && !skip.contains(&k))
            .min_by_key(|&(k, p)| {
                let gap = (p.tile.0 - at.0).abs().max((p.tile.1 - at.1).abs());
                (gap, k)
            })
            .map(|(k, _)| k)
    }

    /// Set worker `h` to work the plans.
    pub(crate) fn hire(&mut self, h: RegimentHandle) {
        if !self.workers.contains(&h) {
            self.workers.push(h);
        }
    }

    /// Regiment slot `r` was given another order (or is gone): it leaves
    /// the plans.
    pub(crate) fn forget(&mut self, r: usize) {
        self.workers.retain(|h| h.index as usize != r);
    }

    pub(crate) fn workers(&self) -> &[RegimentHandle] {
        &self.workers
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.plans.len() as u32);
        for p in &self.plans {
            h.u8(p.owner);
            h.u16(p.building);
            h.i32(p.tile.0);
            h.i32(p.tile.1);
            for v in p.line {
                h.u16(v as u16);
            }
        }
        for w in &self.workers {
            if regs.resolve(*w).is_some() {
                h.u32(w.index);
                h.u16(w.generation);
            }
        }
    }
}

impl Conquest {
    /// `player` plans a wall of type `building` from tile `from` to tile
    /// `to` (see [`Conquest::blueprint`]): nothing is paid or laid yet.
    /// Refused if any tile couldn't go up now, or there is nothing new to
    /// plan.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plan_wall(
        &mut self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> bool {
        let Ok(plan) = self.blueprint(player, building, from, to, nav, regs, terrain) else {
            return false;
        };
        if plan.is_empty() || plan.iter().any(|t| t.fits.is_err()) {
            return false;
        }
        let line = [from.0, from.1, to.0, to.1].map(|v| v as i16);
        for t in plan {
            self.blueprints.add(Plan {
                owner: player,
                building,
                tile: t.tile,
                line,
            });
        }
        true
    }

    /// `player` drops their line planned from `from` to `to` (either way
    /// round), or, when the two are the same tile, their plan on that
    /// tile; tiles planned in other lines stay. Refused if there was none.
    pub(crate) fn drop_plan(&mut self, player: PlayerId, from: (i32, i32), to: (i32, i32)) -> bool {
        let line = |a: (i32, i32), b: (i32, i32)| [a.0, a.1, b.0, b.1].map(|v| v as i16);
        let (there, back) = (line(from, to), line(to, from));
        let tile = (from == to).then_some(from);
        self.blueprints.drop_where(|p| {
            p.owner == player && (p.line == there || p.line == back || Some(p.tile) == tile)
        })
    }

    /// Worker slot `r`, `player`'s, is set to work `player`'s plans.
    /// Refused for a unit that isn't a worker, one used up on what it
    /// builds, or a side with nothing planned.
    pub(crate) fn work_plans(&mut self, player: PlayerId, r: usize, regs: &Regiments) -> bool {
        let kind = self.unit(regs.unit[r]);
        if !kind.worker
            || kind.consumed
            || !self.blueprints.plans().iter().any(|p| p.owner == player)
        {
            return false;
        }
        self.stop_work(r);
        self.blueprints.hire(regs.handle(r));
        true
    }

    /// A tick of the workers on the plans: each with nothing else to do
    /// takes the nearest planned tile of its side's, lays it and goes to
    /// build it, paying for it then; it waits while its side can't pay. A
    /// tile that can no longer go up is dropped, and a worker whose side
    /// has nothing left planned leaves the plans.
    pub(crate) fn tend_plans(&mut self, nav: &mut Nav, regs: &mut Regiments, terrain: &Terrain) {
        if self.blueprints.workers.is_empty() {
            return;
        }
        let mut workers = core::mem::take(&mut self.blueprints.workers);
        workers.retain(|&h| {
            let Some(r) = regs.resolve(h) else {
                return false;
            };
            if self.has_job(regs, r) {
                return true;
            }
            // A worker in melee or routing is left to it.
            if regs.engaged[r].is_some() || regs.morale_state[r].broken() {
                return true;
            }
            let player = regs.owner[r];
            let at = regs.position[r];
            let at = (at.x.floor_int(), at.y.floor_int());
            let mut tried = [usize::MAX; TRIES];
            for k in 0..TRIES {
                let Some(n) = self.blueprints.nearest(player, at, &tried[..k]) else {
                    // Nothing left planned (or only crowded tiles).
                    return k > 0;
                };
                tried[k] = n;
                let plan = self.blueprints.plans[n];
                // A side that can't pay, or has lost the tier for now, waits;
                // paying is checked first, so waiting costs little each tick.
                let b = match self.may_build(player, plan.building) {
                    Ok(b) => b,
                    Err(Refusal::Tier) => return true,
                    Err(_) => {
                        self.blueprints.take(n);
                        return true;
                    }
                };
                let (grub, marrow) = (u32::from(b.cost_grub), u32::from(b.cost_marrow));
                if !self.economy.can_afford(player, grub, marrow) {
                    return true;
                }
                let fits =
                    self.wall_tile_fits(player, plan.building, plan.tile, nav, regs, terrain);
                match fits {
                    Ok(()) => {}
                    // Someone standing by it: try the next nearest.
                    Err(Refusal::Crowded) => continue,
                    Err(_) => {
                        // The ground was taken meanwhile.
                        self.blueprints.take(n);
                        return true;
                    }
                }
                let tile = WallTile {
                    tile: plan.tile,
                    fits,
                };
                let Some((h, _)) = self.structures.found_wall(
                    player,
                    plan.building,
                    &[tile],
                    plan.line,
                    nav,
                    regs,
                ) else {
                    // No room left for buildings: it leaves the plans.
                    return false;
                };
                self.blueprints.take(n);
                self.economy.spend(player, grub, marrow);
                self.structures.send(regs, r, h, Work::Build);
                return true;
            }
            true
        });
        self.blueprints.workers = workers;
    }
}
