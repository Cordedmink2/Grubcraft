//! Faction powers (docs/17-unit-rosters.md, the faction meters' abilities):
//! what a player spends their faction meter on.
//!
//! A power costs meter points, needs its caster's tier, and can't be used
//! again until its cooldown has passed. Only the Choir's Resonance pays for
//! powers so far (see [`crate::chords`]):
//!
//! - **Haste** (Overclock): one of the caster's buildings or regiments works
//!   [`PowerEffect::Haste`]'s percent faster for the power's duration: a
//!   building trains, upgrades, is built and tunes faster, and a regiment
//!   reloads, strikes and builds faster. Used again on the same target, the
//!   new haste replaces the old.
//! - **Shift** (Phase Shift): the caster's chosen regiments step straight
//!   to one of their finished buildings of the power's type, round it,
//!   leaving any fight. Every [`Command::UsePower`](crate::Command::UsePower)
//!   shifting a regiment to the same building within [`RIDE_TICKS`] of the
//!   payment rides on it, so a group order is one use even when the network
//!   carries it over a few turns.
//!
//! The powers table and when each player last used each power are in the
//! state hash; a haste's ticks left are, with the building or regiment.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::hash::StateHasher;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{BuildingId, Refusal, StructureHandle, Structures};
use crate::{MAX_PARTIES, PlayerId, RegimentHandle};

/// The longest a power's cooldown or haste lasts, in ticks (10 minutes).
pub const MAX_POWER_TICKS: u32 = 10 * 60 * crate::TICKS_PER_SECOND;
/// The most percent faster a haste makes anything.
pub const MAX_HASTE: u8 = 100;
/// The most powers a match's table holds.
pub const MAX_POWERS: usize = 64;
/// How far from the edge of the building a shifted regiment stands, in
/// tiles, and how much further each ring out.
const SHIFT_OUT: i32 = 3;
const SHIFT_RING: i32 = 4;
/// Rings of eight spots round the building a shift fills, nearest first.
const SHIFT_RINGS: i32 = 5;
/// How long after a shift is paid for more regiments shifted to the same
/// building ride on it, in ticks (a second).
pub const RIDE_TICKS: u32 = crate::TICKS_PER_SECOND;

/// What a power does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerEffect {
    /// Not built yet: it can't be used.
    #[default]
    NotBuilt,
    /// A building or regiment works this many percent faster.
    Haste { percent: u8 },
    /// Regiments step to one of the caster's buildings of this type.
    Shift { to: BuildingId },
}

/// What the simulation needs from a power. The game builds these from the
/// baked powers table (`gc_data`); tests build them by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerStats {
    /// Main-building tier needed to use it.
    pub tier: u8,
    /// Resonance points it costs.
    pub cost: u8,
    pub cooldown_ticks: u32,
    /// How long a haste lasts.
    pub duration_ticks: u32,
    pub effect: PowerEffect,
}

impl PowerStats {
    /// The same power, with every value in the range the simulation works
    /// with: times up to [`MAX_POWER_TICKS`], a haste of 1 to
    /// [`MAX_HASTE`] percent lasting at least a tick.
    pub fn sanitized(self) -> PowerStats {
        let effect = match self.effect {
            PowerEffect::Haste { percent } => PowerEffect::Haste {
                percent: percent.clamp(1, MAX_HASTE),
            },
            other => other,
        };
        let duration = match effect {
            PowerEffect::Haste { .. } => self.duration_ticks.clamp(1, MAX_POWER_TICKS),
            _ => 0,
        };
        PowerStats {
            cooldown_ticks: self.cooldown_ticks.min(MAX_POWER_TICKS),
            duration_ticks: duration,
            effect,
            ..self
        }
    }

    fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.tier);
        h.u8(self.cost);
        h.u32(self.cooldown_ticks);
        h.u32(self.duration_ticks);
        match self.effect {
            PowerEffect::NotBuilt => h.u8(0),
            PowerEffect::Haste { percent } => {
                h.u8(1);
                h.u8(percent);
            }
            PowerEffect::Shift { to } => {
                h.u8(2);
                h.u16(to);
            }
        }
    }
}

/// What a [`Command::UsePower`](crate::Command::UsePower) is used on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerTarget {
    /// One of the caster's buildings (a haste).
    Structure(StructureHandle),
    /// One of the caster's regiments (a haste).
    Regiment(RegimentHandle),
    /// One of the caster's regiments, to step to their building `to` (a
    /// shift).
    Shift {
        regiment: RegimentHandle,
        to: StructureHandle,
    },
}

/// The powers table and each player's use of it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Powers {
    stats: Vec<PowerStats>,
    /// By player, then power: the tick it was last used, if ever.
    used_at: Vec<Option<u32>>,
    /// By player: the building their last paid shift went to, and how
    /// many regiments have gone there on that payment, to spread them
    /// round it.
    shift_to: [Option<StructureHandle>; MAX_PARTIES],
    shifted: [u8; MAX_PARTIES],
}

impl Powers {
    pub(crate) fn new(stats: &[PowerStats]) -> Powers {
        let stats: Vec<PowerStats> = (stats.iter().take(MAX_POWERS))
            .map(|p| p.sanitized())
            .collect();
        Powers {
            used_at: vec![None; stats.len() * MAX_PARTIES],
            stats,
            shift_to: [None; MAX_PARTIES],
            shifted: [0; MAX_PARTIES],
        }
    }

    pub(crate) fn stats(&self) -> &[PowerStats] {
        &self.stats
    }

    fn slot(&self, player: PlayerId, power: u8) -> Option<usize> {
        let (p, k) = (usize::from(player), usize::from(power));
        (p < MAX_PARTIES && k < self.stats.len()).then_some(p * self.stats.len() + k)
    }

    /// Ticks until `player` can use `power` again at `now`: 0 when ready.
    pub(crate) fn cooling(&self, player: PlayerId, power: u8, now: u32) -> u32 {
        let Some((slot, stats)) = self
            .slot(player, power)
            .zip(self.stats.get(usize::from(power)))
        else {
            return 0;
        };
        self.used_at[slot].map_or(0, |at| (at + stats.cooldown_ticks).saturating_sub(now))
    }

    /// Whether `player` could use `power` at `now`, and if not, why: built,
    /// their tier, its cooldown and its price.
    pub(crate) fn can_use(
        &self,
        structures: &Structures,
        player: PlayerId,
        power: u8,
        now: u32,
    ) -> Result<PowerStats, Refusal> {
        let stats = *self
            .stats
            .get(usize::from(power))
            .filter(|s| s.effect != PowerEffect::NotBuilt)
            .ok_or(Refusal::Unknown)?;
        self.slot(player, power).ok_or(Refusal::Unknown)?;
        if structures.tier_of(player) < stats.tier {
            return Err(Refusal::Tier);
        }
        if self.cooling(player, power, now) > 0 {
            return Err(Refusal::Cooling);
        }
        if structures.resonance(player) < u32::from(stats.cost) {
            return Err(Refusal::Price);
        }
        Ok(stats)
    }

    /// Whether a shift of `player`'s to `to` with `power` at `now` rides on
    /// one they paid for: to the same building, within [`RIDE_TICKS`].
    fn rides(&self, player: PlayerId, power: u8, to: StructureHandle, now: u32) -> bool {
        let paid = self.slot(player, power).and_then(|slot| self.used_at[slot]);
        paid.is_some_and(|at| now.wrapping_sub(at) < RIDE_TICKS)
            && self.shift_to.get(usize::from(player)) == Some(&Some(to))
    }

    /// `player` uses `power` on `target` at `now`; false (and nothing
    /// spent) if they can't.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn use_power(
        &mut self,
        structures: &mut Structures,
        regs: &mut Regiments,
        nav: &Nav,
        player: PlayerId,
        power: u8,
        target: PowerTarget,
        now: u32,
    ) -> bool {
        let riding = match target {
            PowerTarget::Shift { to, .. } => self.rides(player, power, to, now),
            _ => false,
        };
        let stats = match self.stats.get(usize::from(power)) {
            Some(&s) if riding => s,
            _ => match self.can_use(structures, player, power, now) {
                Ok(s) => s,
                Err(_) => return false,
            },
        };
        let done = match (stats.effect, target) {
            (PowerEffect::Haste { percent }, PowerTarget::Structure(h)) => {
                let ticks = haste_ticks(stats.duration_ticks);
                structures
                    .resolve(h)
                    .filter(|&i| structures.owner_of(i) == player)
                    .map(|i| structures.haste(i, percent, ticks))
                    .is_some()
            }
            (PowerEffect::Haste { percent }, PowerTarget::Regiment(h)) => regs
                .resolve(h)
                .filter(|&r| regs.owner[r] == player && regs.alive[r])
                .map(|r| {
                    regs.haste_left[r] = haste_ticks(stats.duration_ticks);
                    regs.haste[r] = percent;
                })
                .is_some(),
            (PowerEffect::Shift { to: building }, PowerTarget::Shift { regiment, to }) => {
                let p = usize::from(player);
                let before = if riding { self.shifted[p] } else { 0 };
                let moved = shift(
                    structures, regs, nav, player, regiment, building, to, before,
                );
                if moved {
                    self.shift_to[p] = Some(to);
                    self.shifted[p] = before.saturating_add(1);
                }
                moved
            }
            _ => false,
        };
        if done && !riding {
            structures.spend_resonance(player, u32::from(stats.cost));
            if let Some(slot) = self.slot(player, power) {
                self.used_at[slot] = Some(now);
            }
        }
        done
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.stats.len() as u32);
        for s in &self.stats {
            s.hash_into(h);
        }
        for at in &self.used_at {
            h.u32(at.map_or(u32::MAX, |t| t));
        }
        for (to, &n) in self.shift_to.iter().zip(&self.shifted) {
            h.u32(to.map_or(u32::MAX, |t| t.index));
            h.u32(to.map_or(u32::MAX, |t| u32::from(t.generation)));
            h.u8(n);
        }
    }
}

/// Step `player`'s regiment out of any fight (its foes let it go) to the
/// spot after the first `before` round their finished building `to` of
/// type `building`.
#[allow(clippy::too_many_arguments)]
fn shift(
    structures: &Structures,
    regs: &mut Regiments,
    nav: &Nav,
    player: PlayerId,
    regiment: RegimentHandle,
    building: BuildingId,
    to: StructureHandle,
    before: u8,
) -> bool {
    let Some(r) = regs
        .resolve(regiment)
        .filter(|&r| regs.owner[r] == player && regs.alive[r] && !regs.morale_state[r].broken())
    else {
        return false;
    };
    let Some(s) = structures.resolve(to).filter(|&s| {
        structures.owner_of(s) == player
            && structures.finished(s)
            && structures.building_of(s) == building
    }) else {
        return false;
    };
    let foot = structures.footprint(s);
    let centre = structures.centre(s);
    // The spots round the building, nearest ring first, eight to a ring.
    let spots = (0..SHIFT_RINGS).flat_map(|ring| {
        let out = foot[2].max(foot[3]) / 2 + SHIFT_OUT + ring * SHIFT_RING;
        [
            (0, 1),
            (1, 1),
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
        ]
        .map(|(x, y)| centre + FxVec2::new(Fx::from_int(x * out), Fx::from_int(y * out)))
    });
    let Some(at) = spots
        .filter(|&at| nav.walkable(at))
        .nth(usize::from(before))
    else {
        return false;
    };
    let facing = regs.facing[r];
    regs.place(regiment, at, facing, 0).is_some_and(|i| {
        regs.vanish_from_fight(i);
        true
    })
}

/// A haste's ticks, as a building's or regiment's count of them.
fn haste_ticks(duration: u32) -> u16 {
    u16::try_from(duration).unwrap_or(u16::MAX)
}

/// How many steps of work something hasted `percent` percent with `left`
/// ticks of haste to go does this tick: 1, and one more on the ticks that
/// add up to `percent` percent over the haste.
pub(crate) fn steps(left: u16, percent: u8) -> u16 {
    if left == 0 {
        return 1;
    }
    let (left, p) = (u32::from(left), u32::from(percent.min(MAX_HASTE)));
    // At most 2: a haste is at most 100 percent.
    1 + (left * p / 100 - (left - 1) * p / 100) as u16
}

/// Whether something whose turns fall on multiples of the strike interval,
/// hasted `percent` percent, has an extra turn at `t`: halfway through turn
/// `k`, each time the turns so far times the percent pass another hundred.
fn extra_turn(t: u64, percent: u8) -> bool {
    let interval = u64::from(crate::combat::STRIKE_INTERVAL);
    if t % interval != interval / 2 {
        return false;
    }
    let (k, p) = (t / interval, u64::from(percent.min(MAX_HASTE)));
    (k + 1) * p / 100 != k * p / 100
}

impl Structures {
    /// Building slot `i` works `percent` percent faster for `ticks` ticks.
    pub(crate) fn haste(&mut self, i: usize, percent: u8, ticks: u16) {
        self.haste_left[i] = ticks;
        self.haste_percent[i] = percent;
    }

    /// The steps of work building slot `i` does this tick (see [`steps`]).
    pub(crate) fn steps(&self, i: usize) -> u32 {
        u32::from(steps(self.haste_left[i], self.haste_percent[i]))
    }

    /// Building `h`'s haste: ticks left, 0 for none.
    pub fn haste_left(&self, h: StructureHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.haste_left[i])
    }

    /// A tick of every haste goes by.
    pub(crate) fn wear_haste(&mut self) {
        for left in &mut self.haste_left {
            *left = left.saturating_sub(1);
        }
    }
}

impl Regiments {
    /// The steps of work regiment slot `i` does this tick (see [`steps`]).
    pub(crate) fn steps(&self, i: usize) -> u16 {
        steps(self.haste_left[i], self.haste[i])
    }

    /// Whether hasted regiment slot `i` has an extra turn to strike at
    /// `tick`: as many more turns as it is hasted percent, halfway between
    /// its own, so the two never fall together.
    pub(crate) fn extra_turn(&self, i: usize, tick: u32) -> bool {
        self.haste_left[i] > 0 && extra_turn(u64::from(tick) + i as u64, self.haste[i])
    }

    /// A tick of every regiment's haste goes by.
    pub(crate) fn wear_haste(&mut self) {
        for left in &mut self.haste_left {
            *left = left.saturating_sub(1);
        }
    }

    /// Regiment `h`'s haste: ticks left, 0 for none.
    pub fn haste_left(&self, h: RegimentHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.haste_left[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fifty_percent_haste_takes_three_steps_every_two_ticks() {
        let total: u32 = (1..=400).rev().map(|left| u32::from(steps(left, 50))).sum();
        assert_eq!(total, 600);
        assert_eq!(steps(0, 50), 1);
        let total: u32 = (1..=7).rev().map(|left| u32::from(steps(left, 100))).sum();
        assert_eq!(total, 14);
    }

    #[test]
    fn a_haste_adds_its_percent_of_strike_turns_and_never_on_a_turn_of_its_own() {
        let interval = u64::from(crate::combat::STRIKE_INTERVAL);
        for percent in [1, 30, 40, 50, 60, 75, 100] {
            let turns = 100 * interval;
            let extra: Vec<u64> = (0..turns).filter(|&t| extra_turn(t, percent)).collect();
            assert_eq!(extra.len(), usize::from(percent), "{percent}%");
            assert!(extra.iter().all(|t| t % interval != 0), "{percent}%");
        }
    }

    #[test]
    fn sanitizing_keeps_times_and_haste_in_range() {
        let s = PowerStats {
            tier: 1,
            cost: 20,
            cooldown_ticks: u32::MAX,
            duration_ticks: 0,
            effect: PowerEffect::Haste { percent: 250 },
        }
        .sanitized();
        assert_eq!(s.cooldown_ticks, MAX_POWER_TICKS);
        assert_eq!(s.duration_ticks, 1);
        assert_eq!(s.effect, PowerEffect::Haste { percent: MAX_HASTE });
    }
}
