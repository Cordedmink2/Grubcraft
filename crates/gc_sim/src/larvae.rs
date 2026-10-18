//! The Brood's larvae (docs/04-economy.md, docs/03-factions.md): every
//! Brood regiment and worker hatches from one.
//!
//! - A finished building with the [`LARVAE`] flag (the Hive at every tier,
//!   and the Hatchery Pit) holds up to [`LARVA_CAP`] larvae and grows one
//!   every [`LARVA_TICKS`] while it holds fewer, at the match's tempo.
//! - A building put up whole (the opening's Hive) starts full; one built
//!   from a foundation starts empty once finished. A tier upgrade keeps its
//!   larvae. A building that falls takes its larvae with it.
//! - Queueing a unit that hatches ([`crate::ConquestUnit::hatches`]) takes
//!   a larva: from the building it is queued at, if that holds any, else
//!   from whichever of its owner's buildings holds the most (the first, on
//!   a tie). Taking it back off the queue returns the larva to whichever
//!   building with room holds the most.
//!
//! The larvae are kept with the buildings that hold them
//! ([`Structures`]), so they come and go with them.

use crate::PlayerId;
use crate::TICKS_PER_SECOND;
use crate::structures::{LARVAE, Structures};

/// Larvae one building holds.
pub const LARVA_CAP: u8 = 3;
/// Ticks to grow a larva at Standard tempo.
pub const LARVA_TICKS: u32 = 12 * TICKS_PER_SECOND;

impl Structures {
    /// Larvae a building of type flags `flags` starts with when put up
    /// whole.
    pub(crate) fn whole_larvae(flags: u16) -> u8 {
        if flags & LARVAE != 0 { LARVA_CAP } else { 0 }
    }

    /// One tick: every finished larvae building with room grows its next
    /// larva.
    pub(crate) fn grow_larvae(&mut self) {
        let grow = self.tempo_ticks(LARVA_TICKS);
        for i in 0..self.larvae.len() {
            if !self.stands(i) || self.flags_of(i) & LARVAE == 0 || !self.finished(i) {
                continue;
            }
            if self.larvae[i] >= LARVA_CAP {
                self.larva_clock[i] = 0;
                continue;
            }
            if self.larva_clock[i] == 0 {
                self.larva_clock[i] = grow;
            }
            self.larva_clock[i] -= 1;
            if self.larva_clock[i] == 0 {
                self.larvae[i] += 1;
            }
        }
    }

    /// Larvae in building slot `i`, if it is one that holds them.
    pub(crate) fn larvae_in(&self, i: usize) -> Option<u8> {
        (self.flags_of(i) & LARVAE != 0).then_some(self.larvae[i])
    }

    /// `owner`'s larvae ready, and how many their buildings can hold.
    pub(crate) fn larvae_ready(&self, owner: PlayerId) -> (u32, u32) {
        self.finished_with(owner, LARVAE)
            .fold((0, 0), |(held, room), i| {
                (
                    held + u32::from(self.larvae[i]),
                    room + u32::from(LARVA_CAP),
                )
            })
    }

    /// Whether `owner` has a larva ready.
    pub(crate) fn has_larva(&self, owner: PlayerId) -> bool {
        self.finished_with(owner, LARVAE)
            .any(|i| self.larvae[i] > 0)
    }

    /// Whether slot `at` is one of `owner`'s finished larvae buildings.
    fn larva_home(&self, owner: PlayerId, at: usize) -> bool {
        self.stands(at)
            && self.owner_of(at) == owner
            && self.finished(at)
            && self.flags_of(at) & LARVAE != 0
    }

    /// Take one of `owner`'s larvae, from building slot `at` if it holds
    /// any; the caller has checked [`Structures::has_larva`].
    pub(crate) fn take_larva(&mut self, owner: PlayerId, at: usize) {
        let from = Some(at)
            .filter(|&i| self.larva_home(owner, i) && self.larvae[i] > 0)
            .or_else(|| {
                // The most held, the first on a tie.
                self.finished_with(owner, LARVAE)
                    .filter(|&i| self.larvae[i] > 0)
                    .max_by_key(|&i| (self.larvae[i], core::cmp::Reverse(i)))
            });
        if let Some(i) = from {
            self.larvae[i] -= 1;
        }
    }

    /// Give one larva back to `owner`: to whichever of their buildings with
    /// room holds the most (the first, on a tie), so taking one and giving
    /// it back can't set an extra building growing. Lost if none has room.
    pub(crate) fn give_back_larva(&mut self, owner: PlayerId) {
        let to = self
            .finished_with(owner, LARVAE)
            .filter(|&i| self.larvae[i] < LARVA_CAP)
            .max_by_key(|&i| (self.larvae[i], core::cmp::Reverse(i)));
        if let Some(i) = to {
            self.larvae[i] += 1;
        }
    }
}
