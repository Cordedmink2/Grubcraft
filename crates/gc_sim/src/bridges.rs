//! Bridges (docs/26-fortifications.md, "Bridges"): a faction's bridge is
//! drawn across a river like a wall, a tile at a time, by workers who work
//! out from the bank. A finished tile opens its ground on the walkable grid
//! to everyone, over deep water or a flood (`Nav::set_bridged_all`), and
//! nobody wades on it. It falls to blows like any building, and whoever
//! stands on a tile that falls drops into the river: hurt, in disorder,
//! and out on the nearest bank.

use alloc::vec::Vec;

use crate::conquest::Conquest;
use crate::conquest::overlaps;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{LINE, Refusal, Structures};
use crate::terrain::Terrain;
use crate::walls::WallTile;
use crate::{PlayerId, UnitStats};

/// Hit points each soldier on a bridge tile that falls loses: as for a
/// wall that falls under its men.
pub const DROP_HP: u32 = crate::manning::FALL_HP;

impl Structures {
    /// Whether building slot `i` is a bridge tile.
    pub(crate) fn is_bridge(&self, i: usize) -> bool {
        self.stats_of(i).is_bridge()
    }

    /// Whether a builder can reach unfinished bridge slot `j`: it is an
    /// end of the line it was drawn in (on land), or next to a finished
    /// tile of a bridge.
    pub(crate) fn reachable_deck(&self, j: usize) -> bool {
        let [x, y, _, _] = self.footprint(j);
        let [x0, y0, x1, y1] = self.line[j].map(i32::from);
        if (x, y) == (x0, y0) || (x, y) == (x1, y1) {
            return true;
        }
        self.standing().any(|k| {
            let [kx, ky, _, _] = self.footprint(k);
            (kx - x).abs() + (ky - y).abs() == 1 && self.is_bridge(k) && self.finished(k)
        })
    }
}

/// Bring the ground up to date with the bridge tiles finished and fallen
/// since last time (`changes`, drained): the walkable grid and the water
/// on it, and those standing on a tile that fell drop into the river and
/// come out on the nearest bank.
pub(crate) fn settle_decks(
    changes: &mut Vec<([i32; 4], bool)>,
    nav: &mut Nav,
    terrain: &mut Terrain,
    regs: &mut Regiments,
    units: &[UnitStats],
) {
    if changes.is_empty() {
        return;
    }
    for on in [true, false] {
        let feet: Vec<[i32; 4]> = (changes.iter())
            .filter(|c| c.1 == on)
            .map(|c| c.0)
            .collect();
        if feet.is_empty() {
            continue;
        }
        if on {
            // Built over, a tile is a wall on the grid until it is
            // finished (see `Structures::found`).
            nav.set_built_all(&feet, false, regs);
        }
        nav.set_bridged_all(&feet, on, regs);
        for &[x, y, _, _] in &feet {
            terrain.set_deck(x, y, on);
        }
        if on {
            continue;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.on_wall[i] {
                continue;
            }
            let p = regs.position[i];
            let at = [p.x.floor_int(), p.y.floor_int(), 1, 1];
            if !feet.iter().any(|&f| overlaps(f, at)) || nav.walkable(regs.owner[i], p) {
                continue;
            }
            regs.wound_last(i, usize::from(regs.models[i]), DROP_HP);
            regs.disorder(i);
            let mut bank = nav.settle(regs.owner[i], p, regs.berth(units, i));
            if !nav.walkable(regs.owner[i], bank) {
                bank = nav.settle(regs.owner[i], p, 0);
            }
            regs.position[i] = bank;
            regs.halt(i);
        }
    }
    changes.clear();
}

impl Conquest {
    /// The new tiles of `player`'s bridge drawn along `tiles` (in the box
    /// `area`), each with whether it can go up there: a straight line (a
    /// corner step is too narrow to cross) over some water, each end on
    /// dry land a regiment can walk on or on a bridge of `player`'s, so a
    /// gap in one can be drawn again from deck to deck. Tiles where
    /// `player` already has a bridge are left out, as walls leave out
    /// their own tiles, and every other tile must be water or open ground
    /// with nothing built or bridged on it.
    pub(crate) fn bridge_line(
        &self,
        player: PlayerId,
        tiles: &[(i32, i32)],
        area: [i32; 4],
        nav: &Nav,
        terrain: &Terrain,
    ) -> Result<Vec<WallTile>, Refusal> {
        let (Some(&first), Some(&last)) = (tiles.first(), tiles.last()) else {
            return Err(Refusal::Unknown);
        };
        if first.0 != last.0 && first.1 != last.1 {
            return Err(Refusal::Crooked);
        }
        let s = &self.structures;
        let own: Vec<[i32; 4]> = (s.own_in(player, LINE, area))
            .filter(|&i| s.is_bridge(i))
            .map(|i| s.footprint(i))
            .collect();
        let owned = |(x, y): (i32, i32)| own.iter().any(|&f| overlaps(f, [x, y, 1, 1]));
        // The map's own water, whatever the weather or bridges on it.
        let water = |(x, y): (i32, i32)| terrain.tile_under_water(x, y);
        let ground = |(x, y): (i32, i32)| nav.walkable_ground(x, y);
        let bank = |t: (i32, i32)| owned(t) || (ground(t) && !water(t));
        if !bank(first) || !bank(last) || !tiles.iter().any(|&t| !owned(t) && water(t)) {
            return Err(Refusal::Rough);
        }
        Ok(tiles
            .iter()
            .filter(|&&t| !owned(t))
            .map(|&tile| WallTile {
                tile,
                // Unfinished bridges count as built (see `settle_decks`).
                fits: nav.deck_footing(tile.0, tile.1).and_then(|()| {
                    if water(tile) || ground(tile) {
                        Ok(())
                    } else {
                        Err(Refusal::Rough)
                    }
                }),
            })
            .collect())
    }
}
