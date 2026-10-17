//! Power and tuning (docs/03-factions.md, the Choir's economy, and
//! docs/21-buildings.md): the fields that let the Choir's buildings work,
//! and the buildings that work nodes with no workers.
//!
//! - A finished building with the [`POWERS_FIELD`] flag powers a field its
//!   `field` of tiles from its edge (the Spire 8 at every tier, a Resonator
//!   or Siphon 6, in the shipped rosters).
//! - A building with the [`NEEDS_POWER`] flag can only be laid where one of
//!   its owner's fields reaches, and works only while one does: unpowered,
//!   it trains nothing until power returns (its queue waits).
//! - A finished building that tunes ([`crate::structures::BuildingStats::tunes`],
//!   a Resonator for seeps and a Siphon for veins) works every node of its
//!   kind within its reach with no workers (see [`crate::economy`]). A node
//!   takes only the tuner whose centre is nearest it.
//!
//! Which buildings power and tune is worked out afresh each tick from the
//! buildings standing, so it is not stored or hashed; the economy hands
//! nodes to tuners again only when the tuners change.

use gc_fixed::FxVec2;

use crate::PlayerId;
use crate::economy::Resource;
use crate::structures::{NEEDS_POWER, POWERS_FIELD, StructureHandle, Structures};
use crate::veins::{foot_reaches, within};

/// A finished building powering a field, as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldView {
    pub owner: PlayerId,
    pub building: StructureHandle,
    /// Its footprint, `[x, y, width, depth]` in tiles.
    pub foot: [i32; 4],
    /// How far its field reaches from its edge, in tiles.
    pub reach: i32,
}

impl Structures {
    /// Work out this tick's powering and tuning buildings.
    pub(crate) fn link_power(&mut self) {
        self.powerers.clear();
        core::mem::swap(&mut self.tuners, &mut self.tuners_was);
        self.tuners.clear();
        core::mem::swap(&mut self.linkers, &mut self.linkers_was);
        self.linkers.clear();
        for i in 0..self.slots() {
            if !self.stands(i) || !self.finished(i) {
                continue;
            }
            let b = *self.stats_of(i);
            if b.flags & POWERS_FIELD != 0 && b.field > 0 {
                self.powerers.push(self.handle(i));
            }
            if b.tunes.is_some() {
                self.tuners.push(self.handle(i));
            }
            if b.links > 0 {
                self.linkers.push(self.handle(i));
            }
        }
        // By owner, so each owner's lie together for the chords.
        let mut linkers = core::mem::take(&mut self.linkers);
        linkers.sort_unstable_by_key(|h| (self.owner_of(h.index as usize), h.index));
        self.linkers = linkers;
        let relink = core::mem::take(&mut self.restats) || self.linkers != self.linkers_was;
        if relink {
            self.find_chords();
        }
        if relink || self.tuners != self.tuners_was {
            self.power_stamp = self.power_stamp.wrapping_add(1);
        }
    }

    /// The slots of this tick's powering buildings still standing (a slot
    /// freed since, and reused, is not one of them).
    fn fields(&self) -> impl Iterator<Item = usize> + '_ {
        self.powerers.iter().filter_map(|&h| self.resolve(h))
    }

    /// Whether one of `owner`'s fields of power reaches footprint `foot`
    /// (`[x, y, w, d]`, tiles).
    pub(crate) fn power_reaches(&self, owner: PlayerId, foot: [i32; 4]) -> bool {
        self.fields()
            .filter(|&v| self.owner_of(v) == owner)
            .any(|v| foot_reaches(self.footprint(v), foot, self.stats_of(v).field))
    }

    /// Whether building slot `i` works: it needs no power, or has it.
    pub(crate) fn powered(&self, i: usize) -> bool {
        !self.stands(i)
            || !self.needs_power(i)
            || self.power_reaches(self.owner_of(i), self.footprint(i))
    }

    /// Whether building slot `i` needs power to work.
    pub(crate) fn needs_power(&self, i: usize) -> bool {
        self.stats_of(i).flags & NEEDS_POWER != 0
    }

    /// The finished buildings powering a field, as the game sees them.
    pub fn field_views(&self) -> impl Iterator<Item = FieldView> + '_ {
        self.fields().map(|v| FieldView {
            owner: self.owner_of(v),
            building: self.handle(v),
            foot: self.footprint(v),
            reach: i32::from(self.stats_of(v).field),
        })
    }

    /// Of this tick's tuning buildings still standing that tune `resource`
    /// and reach `point`, the one whose centre is nearest it (the lowest
    /// slot, on a tie).
    pub(crate) fn tuner_of(&self, resource: Resource, point: FxVec2) -> Option<usize> {
        (self.tuners.iter())
            .filter_map(|&h| self.resolve(h))
            .filter(|&v| {
                self.stats_of(v).tunes.is_some_and(|(r, tiles)| {
                    r == resource && within(self.footprint(v), point, i32::from(tiles))
                })
            })
            .min_by_key(|&v| ((self.centre(v) - point).length_sq_raw(), v))
    }
}
