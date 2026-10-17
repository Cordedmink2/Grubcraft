//! Chords and Resonance (docs/03-factions.md, the Choir's economy): the
//! buildings that link, the triangles three of them make, and the meter
//! those triangles charge.
//!
//! - Two of one owner's finished buildings that link
//!   ([`crate::structures::BuildingStats::links`], the Resonator and
//!   Siphon 14 tiles in the shipped rosters) link when their centres are no
//!   further apart than the shorter link of the two.
//! - Any three mutually linked make a [`Chord`]. A seep a chord covers and
//!   one of the chord owner's buildings tunes yields more: from
//!   [`MIN_HARMONY`] percent for a sliver of a triangle to [`MAX_HARMONY`]
//!   for the largest the links allow (equilateral). A seep takes only its
//!   best chord; chords don't stack.
//! - Each tick a player's Resonance charges by their chords, up to
//!   [`MAX_CHARGING_CHORDS`] of them and two fewer than their linking
//!   buildings (as many as triangles that don't overlap, so nested ones
//!   charge no faster): [`RESONANCE_TICKS`] chord-ticks a point, to
//!   [`MAX_RESONANCE`].
//!
//! Which buildings link and the chords they make are worked out again only
//! when the linking buildings change; neither is stored or hashed (each
//! player keeps their first [`MAX_LINKS_EACH`] links and [`MAX_CHORDS_EACH`]
//! chords, in slot order). The Resonance meters are.

use gc_fixed::{Fx, FxVec2};

use crate::structures::{StructureHandle, Structures};
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// At most this many chords are kept for each player, the first found
/// (in slot order).
pub const MAX_CHORDS_EACH: usize = 128;
/// At most this many links are kept for each player (they are only
/// drawn), the first found.
pub const MAX_LINKS_EACH: usize = 256;
/// A seep in the smallest chord yields this many percent more.
pub const MIN_HARMONY: u8 = 10;
/// A seep in the largest chord the links allow yields this many percent
/// more.
pub const MAX_HARMONY: u8 = 60;
/// The Resonance meter's top.
pub const MAX_RESONANCE: u32 = 100;
/// Chord-ticks to a point of Resonance: a point every 3 seconds for one
/// chord.
pub const RESONANCE_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// Chords past this many charge Resonance no faster.
pub const MAX_CHARGING_CHORDS: u32 = 3;

/// Three of one owner's buildings, mutually linked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub owner: PlayerId,
    pub corners: [StructureHandle; 3],
    /// The corners' centres.
    pub at: [FxVec2; 3],
    /// How many percent more a seep it covers yields.
    pub harmony: u8,
}

/// Two of one owner's buildings, linked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub owner: PlayerId,
    pub ends: [StructureHandle; 2],
    /// The ends' centres.
    pub at: [FxVec2; 2],
    /// The harmony of the best chord it is an edge of; 0 for none.
    pub harmony: u8,
}

impl Structures {
    /// Find the links and chords this tick's linking buildings make. The
    /// linkers are sorted by owner, so each owner's lie together.
    pub(crate) fn find_chords(&mut self) {
        self.chords.clear();
        self.links.clear();
        self.chord_count = [0; MAX_PARTIES];
        self.charging = [0; MAX_PARTIES];
        let n = self.linkers.len();
        let mut start = 0;
        while start < n {
            let owner = self.owner_of(self.linker(start));
            let end = (start..n)
                .find(|&k| self.owner_of(self.linker(k)) != owner)
                .unwrap_or(n);
            self.chords_among(owner, start, end);
            start = end;
        }
    }

    /// The links and chords among `owner`'s linkers `start..end`.
    fn chords_among(&mut self, owner: PlayerId, start: usize, end: usize) {
        let (first_chord, first_link) = (self.chords.len(), self.links.len());
        for a in start..end {
            for b in a + 1..end {
                if !self.links_with(a, b) {
                    continue;
                }
                if self.links.len() - first_link < MAX_LINKS_EACH {
                    self.links.push(Link {
                        owner,
                        ends: [self.linkers[a], self.linkers[b]],
                        at: [a, b].map(|k| self.centre(self.linker(k))),
                        harmony: 0,
                    });
                }
                for c in b + 1..end {
                    if self.chords.len() - first_chord < MAX_CHORDS_EACH
                        && self.links_with(a, c)
                        && self.links_with(b, c)
                    {
                        self.add_chord(owner, [a, b, c]);
                    }
                }
            }
        }
        let (chords, links) = (&self.chords[first_chord..], &mut self.links[first_link..]);
        for link in links {
            link.harmony = (chords.iter())
                .filter(|c| link.ends.iter().all(|e| c.corners.contains(e)))
                .map(|c| c.harmony)
                .max()
                .unwrap_or(0);
        }
        let p = usize::from(owner);
        if p >= MAX_PARTIES {
            return;
        }
        let count = u32::try_from(self.chords.len() - first_chord).unwrap_or(u32::MAX);
        // Chords that share edges each charge, but no more than two fewer
        // than the buildings (as many as triangles that don't overlap), so
        // a building added to a cluster charges for one chord at most.
        let most = u32::try_from((end - start).saturating_sub(2)).unwrap_or(u32::MAX);
        self.chord_count[p] = count.min(u32::from(u8::MAX)) as u8;
        self.charging[p] = count.min(most).min(MAX_CHARGING_CHORDS) as u8;
    }

    /// The building slot of this tick's linker `k`.
    fn linker(&self, k: usize) -> usize {
        self.linkers[k].index as usize
    }

    /// Whether this tick's linkers `a` and `b`, of one owner, link: their
    /// centres are within the shorter link of the two.
    fn links_with(&self, a: usize, b: usize) -> bool {
        let (i, j) = (self.linker(a), self.linker(b));
        let reach = self.stats_of(i).links.min(self.stats_of(j).links);
        let reach = u64::from(Fx::from_int(i32::from(reach)).raw().unsigned_abs());
        (self.centre(i) - self.centre(j)).length_sq_raw() <= reach * reach
    }

    /// Keep the chord `owner`'s linkers `k` make, unless its corners lie in
    /// a line.
    fn add_chord(&mut self, owner: PlayerId, k: [usize; 3]) {
        let slots = k.map(|k| self.linker(k));
        let at = slots.map(|i| self.centre(i));
        let twice_area = cross(at[0], at[1], at[2]).unsigned_abs();
        if twice_area == 0 {
            return;
        }
        let reach = (slots.iter())
            .map(|&i| self.stats_of(i).links)
            .min()
            .unwrap_or(0);
        self.chords.push(Chord {
            owner,
            corners: k.map(|k| self.linkers[k]),
            at,
            harmony: harmony_of(twice_area, reach),
        });
    }

    /// How many percent more a seep at `point` that `owner` tunes yields:
    /// its best chord of theirs that covers it, or 0.
    pub(crate) fn harmony_at(&self, owner: PlayerId, point: FxVec2) -> u8 {
        (self.chords.iter())
            .filter(|c| c.owner == owner && covers(c.at, point))
            .map(|c| c.harmony)
            .max()
            .unwrap_or(0)
    }

    /// Charge each player's Resonance by their chords.
    pub(crate) fn charge_resonance(&mut self) {
        let full = MAX_RESONANCE * RESONANCE_TICKS;
        for p in 0..MAX_PARTIES {
            let chords = u32::from(self.charging[p]);
            self.resonance[p] = (self.resonance[p] + chords).min(full);
        }
    }

    /// Take `points` of player `p`'s Resonance (they have them).
    pub(crate) fn spend_resonance(&mut self, p: PlayerId, points: u32) {
        if let Some(r) = self.resonance.get_mut(usize::from(p)) {
            *r = r.saturating_sub(points * RESONANCE_TICKS);
        }
    }

    /// Player `p`'s Resonance, in points.
    pub fn resonance(&self, p: PlayerId) -> u32 {
        (self.resonance.get(usize::from(p))).map_or(0, |&r| r / RESONANCE_TICKS)
    }

    /// This tick's chords (each player's first [`MAX_CHORDS_EACH`]).
    pub fn chords(&self) -> &[Chord] {
        &self.chords
    }

    /// This tick's links (each player's first [`MAX_LINKS_EACH`]).
    pub fn links(&self) -> &[Link] {
        &self.links
    }
}

/// Twice the signed area of triangle `a b c`, in raw units squared.
fn cross(a: FxVec2, b: FxVec2, c: FxVec2) -> i128 {
    let (bx, by) = (i128::from((b.x - a.x).raw()), i128::from((b.y - a.y).raw()));
    let (cx, cy) = (i128::from((c.x - a.x).raw()), i128::from((c.y - a.y).raw()));
    bx * cy - by * cx
}

/// Whether `point` lies in triangle `t`, its edges included.
fn covers(t: [FxVec2; 3], point: FxVec2) -> bool {
    let d = [
        cross(t[0], t[1], point),
        cross(t[1], t[2], point),
        cross(t[2], t[0], point),
    ];
    d.iter().all(|&v| v >= 0) || d.iter().all(|&v| v <= 0)
}

/// The harmony of a chord of twice the area `twice_area` (raw units
/// squared) whose shortest link is `reach` tiles: [`MIN_HARMONY`] up to
/// [`MAX_HARMONY`] at the area of an equilateral triangle of side `reach`.
fn harmony_of(twice_area: u128, reach: u8) -> u8 {
    let one = u128::from(Fx::ONE.raw().unsigned_abs());
    // Twice an equilateral triangle's area is √3/2 of its side squared.
    let side = u128::from(reach) * one;
    let most = (side * side * 866 / 1000).max(1);
    let share = twice_area.min(most) * u128::from(MAX_HARMONY - MIN_HARMONY) / most;
    MIN_HARMONY + share as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn a_point_on_an_edge_or_inside_is_covered_and_one_outside_is_not() {
        let t = [at(0, 0), at(10, 0), at(0, 10)];
        assert!(covers(t, at(2, 2)));
        assert!(covers(t, at(5, 0)));
        assert!(!covers(t, at(6, 6)));
        let back = [t[0], t[2], t[1]];
        assert!(covers(back, at(2, 2)));
    }

    #[test]
    fn harmony_grows_with_area_up_to_the_largest_triangle() {
        let one = u128::from(Fx::ONE.raw().unsigned_abs());
        let unit = one * one;
        assert_eq!(harmony_of(unit, 14), MIN_HARMONY);
        // An equilateral triangle of side 14: twice its area is about 169.7.
        assert_eq!(harmony_of(170 * unit, 14), MAX_HARMONY);
        assert_eq!(harmony_of(1000 * unit, 14), MAX_HARMONY);
        let half = harmony_of(85 * unit, 14);
        assert!((34..=36).contains(&half), "{half}");
    }
}
