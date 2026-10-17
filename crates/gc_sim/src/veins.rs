//! Root-veins (docs/03-factions.md, the Brood's economy): the network that
//! carries what burrowed workers gather home, and that living buildings
//! regrow on.
//!
//! - A finished building with the [`VEINS`] flag spreads root-veins its
//!   `field` of tiles from its edge (the Hive 15 at every tier, a Pustule
//!   8, in the shipped rosters).
//! - The network is what links back to a main building: a main building's
//!   veins are linked, and so are the veins of any veins building standing
//!   within reach of linked veins. Kill a relay and whatever was linked
//!   only through it is cut off until it is regrown.
//! - A worker that burrows ([`crate::ConquestUnit::burrows`], the Gnawer)
//!   stays at its seep or vein and sends each load home through the
//!   network instead of carrying it: a load takes [`VEIN_FLOW_TICKS`]
//!   longer to come in, and none comes in while the node is off its
//!   owner's linked veins (see [`crate::economy`]).
//! - A finished building with Living armour standing on its owner's
//!   linked veins regrows [`REGROW_PER_SECOND`] hit points a second.
//!
//! The links are worked out afresh each tick from the buildings standing,
//! so they are not stored or hashed.

use gc_fixed::{Fx, FxVec2};

use crate::structures::{Armour, Role, StructureHandle, Structures, VEINS, nearest_point};
use crate::{PlayerId, TICKS_PER_SECOND};

/// What a burrowed worker's load adds to its time, flowing home through
/// the veins, at Standard tempo: five Gnawers on a seep bring in about 200
/// Grub a minute, as five walking workers do from seven tiles off.
pub const VEIN_FLOW_TICKS: u32 = 6 * TICKS_PER_SECOND;
/// Hit points a living building on its owner's veins regrows a second.
pub const REGROW_PER_SECOND: u32 = 2;
/// Not linked to the network.
pub(crate) const UNLINKED: u32 = u32::MAX;

/// A veins building linked to its owner's network, as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VeinView {
    pub owner: PlayerId,
    pub building: StructureHandle,
    /// The building it links through: itself, for a main building.
    pub parent: StructureHandle,
    /// Centre of its footprint, and of its parent's.
    pub at: FxVec2,
    pub from: FxVec2,
    /// How far its veins spread from its edge, in tiles.
    pub reach: i32,
}

/// Squared gap between two footprints `[x, y, w, d]`, edge to edge, in
/// tiles.
fn gap_sq([ax, ay, aw, ad]: [i32; 4], [bx, by, bw, bd]: [i32; 4]) -> i64 {
    let dx = i64::from((ax - (bx + bw)).max(bx - (ax + aw)).max(0));
    let dy = i64::from((ay - (by + bd)).max(by - (ay + ad)).max(0));
    dx * dx + dy * dy
}

/// Whether footprints `a` and `b` lie within `tiles` of each other, edge
/// to edge.
pub(crate) fn foot_reaches(a: [i32; 4], b: [i32; 4], tiles: u8) -> bool {
    let reach = i64::from(tiles);
    gap_sq(a, b) <= reach * reach
}

/// Whether `point` lies within `tiles` of footprint `foot`'s edge.
pub(crate) fn within(foot: [i32; 4], point: FxVec2, tiles: i32) -> bool {
    let reach = u64::from(Fx::from_int(tiles).raw().unsigned_abs());
    (nearest_point(foot, point) - point).length_sq_raw() <= reach * reach
}

impl Structures {
    /// How far building slot `i`'s veins reach, if it spreads them: a main
    /// building's further than a relay's.
    fn vein_reach(&self, i: usize) -> Option<u8> {
        let b = self.stats_of(i);
        (b.flags & VEINS != 0 && b.field > 0).then_some(b.field)
    }

    /// Work out which veins buildings link back to a main building: each
    /// linked one's `vein_from` is the slot it links through (its own, for
    /// a main building), and [`UNLINKED`] for the rest; `vein_linked`
    /// lists the linked slots.
    pub(crate) fn link_veins(&mut self) {
        self.vein_from.fill(UNLINKED);
        self.vein_linked.clear();
        self.vein_cands.clear();
        for i in 0..self.vein_from.len() {
            if self.stands(i) && self.finished(i) && self.vein_reach(i).is_some() {
                if self.stats_of(i).role == Role::Main {
                    self.vein_from[i] = i as u32;
                    self.vein_linked.push(i as u32);
                } else {
                    self.vein_cands.push(i as u32);
                }
            }
        }
        // Breadth first from the main buildings: the linked list is its own
        // queue.
        let mut next = 0;
        while let Some(&a) = self.vein_linked.get(next) {
            next += 1;
            let a = a as usize;
            for k in 0..self.vein_cands.len() {
                let b = self.vein_cands[k] as usize;
                if self.vein_from[b] == UNLINKED
                    && self.owner_of(b) == self.owner_of(a)
                    && self.reaches(a, self.footprint(b))
                {
                    self.vein_from[b] = a as u32;
                    self.vein_linked.push(b as u32);
                }
            }
        }
    }

    /// Whether linked veins building `v`'s veins reach footprint `foot`.
    fn reaches(&self, v: usize, foot: [i32; 4]) -> bool {
        foot_reaches(self.footprint(v), foot, self.vein_reach(v).unwrap_or(0))
    }

    /// The slots of the linked veins buildings still standing (one may
    /// have fallen since the links were worked out this tick).
    fn linked(&self) -> impl Iterator<Item = usize> + '_ {
        self.vein_linked
            .iter()
            .map(|&v| v as usize)
            .filter(|&v| self.stands(v))
    }

    /// The slots of `owner`'s linked veins buildings.
    fn veins_of(&self, owner: PlayerId) -> impl Iterator<Item = usize> + '_ {
        self.linked().filter(move |&v| self.owner_of(v) == owner)
    }

    fn vein_view(&self, i: usize) -> VeinView {
        let parent = self.vein_from[i] as usize;
        VeinView {
            owner: self.owner_of(i),
            building: self.handle(i),
            parent: self.handle(parent),
            at: self.centre(i),
            from: self.centre(parent),
            reach: i32::from(self.vein_reach(i).unwrap_or(0)),
        }
    }

    /// The linked veins buildings, as the game sees them.
    pub fn vein_views(&self) -> impl Iterator<Item = VeinView> + '_ {
        self.linked().map(|i| self.vein_view(i))
    }

    /// Of `owner`'s linked veins buildings whose veins reach `point`, the
    /// one whose centre is nearest it (the first, on a tie).
    pub(crate) fn vein_home(&self, owner: PlayerId, point: FxVec2) -> Option<usize> {
        self.veins_of(owner)
            .filter(|&v| {
                within(
                    self.footprint(v),
                    point,
                    i32::from(self.vein_reach(v).unwrap_or(0)),
                )
            })
            .min_by_key(|&v| ((self.centre(v) - point).length_sq_raw(), v))
    }

    /// The linked veins building that carries what is gathered at `point`
    /// home for `owner`, as the game sees it.
    pub fn vein_home_view(&self, owner: PlayerId, point: FxVec2) -> Option<VeinView> {
        self.vein_home(owner, point).map(|v| self.vein_view(v))
    }

    /// Whether `point` lies on `owner`'s linked veins.
    pub(crate) fn on_veins(&self, owner: PlayerId, point: FxVec2) -> bool {
        self.vein_home(owner, point).is_some()
    }

    /// Whether building slot `i` regrows on veins: standing, finished and
    /// Living.
    pub(crate) fn regrows(&self, i: usize) -> bool {
        self.stands(i) && self.finished(i) && self.stats_of(i).armour == Armour::Living
    }

    /// Whether building slot `i` stands on its owner's linked veins.
    pub(crate) fn rooted(&self, i: usize) -> bool {
        let foot = self.footprint(i);
        self.veins_of(self.owner_of(i))
            .any(|v| self.reaches(v, foot))
    }

    /// One tick: hurt living buildings on their owner's veins regrow.
    pub(crate) fn regrow(&mut self) {
        for i in 0..self.vein_from.len() {
            if self.regrows(i) && self.hurt(i) && self.rooted(i) {
                self.heal(i, REGROW_PER_SECOND);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::gap_sq;

    #[test]
    fn footprints_touching_or_overlapping_are_no_gap_apart() {
        assert_eq!(gap_sq([0, 0, 2, 2], [2, 0, 2, 2]), 0);
        assert_eq!(gap_sq([0, 0, 5, 5], [1, 1, 2, 2]), 0);
        assert_eq!(gap_sq([0, 0, 2, 2], [5, 6, 2, 2]), 9 + 16);
        assert_eq!(gap_sq([5, 6, 2, 2], [0, 0, 2, 2]), 9 + 16);
    }
}
