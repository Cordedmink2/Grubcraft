//! Honor (docs/03-factions.md, the Ember Clans' meter): what a player whose
//! meter is [`Meter::Honor`] earns by fighting well and loses by fleeing,
//! from 0 to [`MAX_HONOR`], and spends on Edicts (see [`crate::powers`]).
//!
//! - An enemy regiment routing while one of theirs is locked in melee with
//!   it earns them [`ROUT_HONOR`], or [`OUTNUMBERED_HONOR`] if their
//!   regiments in that fight had fewer soldiers than it.
//! - One of their own regiments routing costs them [`FLED_HONOR`].
//! - Each of their finished buildings that earns Honor (a Shrine) earns a
//!   point every [`SHRINE_TICKS`].
//!
//! Every player's Honor and Shrine clock are in the state hash.

use crate::hash::StateHasher;
use crate::powers::Meter;
use crate::regiments::Regiments;
use crate::structures::{HONOR, Structures};
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// The most Honor a player holds.
pub const MAX_HONOR: u32 = 100;
/// Honor for an enemy routed in a fight with yours.
pub const ROUT_HONOR: u32 = 5;
/// Honor for an enemy routed in a fight where yours were fewer.
pub const OUTNUMBERED_HONOR: u32 = 10;
/// Honor lost when one of yours routs.
pub const FLED_HONOR: u32 = 5;
/// Ticks a Shrine takes to earn a point of Honor (30 s: 2 a minute).
pub const SHRINE_TICKS: u32 = 30 * TICKS_PER_SECOND;

/// Each player's Honor.
#[derive(Clone, Debug, Default)]
pub(crate) struct Honor {
    points: [u8; MAX_PARTIES],
    /// Shrine ticks toward the next point.
    clock: [u32; MAX_PARTIES],
}

impl Honor {
    /// `player`'s Honor.
    pub(crate) fn points(&self, player: PlayerId) -> u32 {
        self.points
            .get(usize::from(player))
            .map_or(0, |&p| u32::from(p))
    }

    /// `player` spends `points` of Honor (at most what they hold).
    pub(crate) fn spend(&mut self, player: PlayerId, points: u32) {
        if let Some(p) = self.points.get_mut(usize::from(player)) {
            *p = u32::from(*p).saturating_sub(points) as u8;
        }
    }

    fn gain(&mut self, player: PlayerId, points: u32) {
        if let Some(p) = self.points.get_mut(usize::from(player)) {
            *p = (u32::from(*p) + points).min(MAX_HONOR) as u8;
        }
    }

    /// The Honor won and lost in tick `tick`, after morale (players whose
    /// meter in `meters` isn't Honor earn none): routs this tick, and once
    /// a second the Shrines.
    pub(crate) fn earn(
        &mut self,
        meters: &[Meter; MAX_PARTIES],
        structures: &Structures,
        regs: &Regiments,
        tick: u32,
    ) {
        if !meters.contains(&Meter::Honor) {
            return;
        }
        let honored = |p: PlayerId| meters.get(usize::from(p)) == Some(&Meter::Honor);
        for i in 0..regs.alive.len() {
            // A rout this tick marks the tick after it.
            if regs.alive[i] && regs.last_rout[i] == tick + 1 {
                self.rout(regs, i, honored);
            }
        }
        if tick.is_multiple_of(TICKS_PER_SECOND) {
            for p in (0..MAX_PARTIES as PlayerId).filter(|&p| honored(p)) {
                let shrines = structures.finished_with(p, HONOR).count() as u32;
                let clock = &mut self.clock[usize::from(p)];
                *clock += shrines * TICKS_PER_SECOND;
                let earned = *clock / SHRINE_TICKS;
                *clock %= SHRINE_TICKS;
                self.gain(p, earned);
            }
        }
    }

    /// Regiment slot `i` routed this tick: its owner loses Honor, and the
    /// players fighting it win some.
    fn rout(&mut self, regs: &Regiments, i: usize, honored: impl Fn(PlayerId) -> bool) {
        let owner = regs.owner[i];
        if honored(owner) {
            self.spend(owner, FLED_HONOR);
        }
        // Its foes still hold it until combat lets go next tick.
        let me = regs.handle(i);
        let mut soldiers = [0u32; MAX_PARTIES];
        for k in 0..regs.alive.len() {
            if regs.alive[k]
                && regs.engaged[k] == Some(me)
                && let Some(n) = soldiers.get_mut(usize::from(regs.owner[k]))
            {
                *n += u32::from(regs.models[k]);
            }
        }
        for (p, &n) in soldiers.iter().enumerate() {
            let p = p as PlayerId;
            if n > 0 && p != owner && honored(p) {
                let fewer = n < u32::from(regs.models[i]);
                self.gain(p, if fewer { OUTNUMBERED_HONOR } else { ROUT_HONOR });
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        for (&p, &c) in self.points.iter().zip(&self.clock) {
            h.u8(p);
            h.u32(c);
        }
    }
}
