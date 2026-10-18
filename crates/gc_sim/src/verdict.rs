//! Who won: one ruling, made by the simulation itself every tick, so the
//! game, the golden replays, the computer players' test harnesses and every
//! machine in a network match agree on it (review fix R5).
//!
//! A side wins by **holding the field**: it still has regiments fighting,
//! and none of its enemies' fighting regiments are in its sight, for
//! [`HOLD_TICKS`] in a row (looked at once a second). Allies share the hold. Once decided, the
//! verdict stands for the rest of the match; the battle itself goes on.
//!
//! Conquest is won otherwise (docs/06-game-modes.md, "Victory conditions"),
//! by whichever of these the lobby turned on ([`Victories`]):
//! - **Annihilation:** a player with no main building left has
//!   [`EXILE_TICKS`] to lay one, or is out: their regiments and buildings
//!   leave the field. The countdown waits while one is going up and ends
//!   when one is finished. When everyone left (those who have had a main
//!   building, and any other with an army) is on one side, that side wins;
//!   when the last of them are put out together, nobody does
//!   ([`Win::Draw`]).
//! - **Titan's Heart:** from [`HEART_UNLOCK_TICKS`] (scaled by tempo), a
//!   side with regiments within [`HEART_REACH`] tiles of the Heart and no
//!   enemy there holds it; holding it for [`HEART_HOLD_TICKS`] in a row
//!   wins.

use gc_fixed::{Fx, FxVec2};

use crate::hash::StateHasher;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::vision::Vision;
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// How long a side must hold the field to win.
pub const HOLD_TICKS: u32 = 10 * TICKS_PER_SECOND;
/// How long a Conquest player with no main building has to lay one.
pub const EXILE_TICKS: u32 = 90 * TICKS_PER_SECOND;
/// When the Titan's Heart can first be held, at Standard tempo: minute 20.
pub const HEART_UNLOCK_TICKS: u32 = 20 * 60 * TICKS_PER_SECOND;
/// How long the Heart must be held, uncontested, to win.
pub const HEART_HOLD_TICKS: u32 = 3 * 60 * TICKS_PER_SECOND;
/// How near the Heart (in tiles) a regiment holds or contests it.
pub const HEART_REACH: Fx = Fx::from_int(6);

/// How a match was won.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Win {
    /// A side held the field.
    Field,
    /// Every enemy is out of a Conquest match.
    Annihilation,
    /// A side held the Titan's Heart.
    Heart,
    /// The last players in a Conquest match were put out together: nobody
    /// won (the verdict's holder is the lowest of them).
    Draw,
}

impl Win {
    pub fn from_u8(v: u8) -> Option<Win> {
        [Win::Field, Win::Annihilation, Win::Heart, Win::Draw]
            .get(usize::from(v))
            .copied()
    }
}

/// How a match was decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The party holding the field, the last standing or holding the
    /// Heart when it was decided (its allies won too).
    pub holder: PlayerId,
    /// The tick it was decided on, counted from the start of the match.
    pub tick: u32,
    pub by: Win,
}

/// The parties with a main building finished (`built`) and with one going
/// up (`rising`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Mains {
    pub(crate) built: u8,
    pub(crate) rising: u8,
}

/// Which victories a Conquest match is played for. With neither, it never
/// ends (the sandbox).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Victories {
    pub annihilation: bool,
    pub heart: bool,
}

impl Victories {
    pub const BOTH: Victories = Victories {
        annihilation: true,
        heart: true,
    };

    /// Whether the match is played for any victory at all.
    pub fn any(self) -> bool {
        self.annihilation || self.heart
    }
}

/// The Titan's Heart as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartView {
    /// Its middle, in tiles.
    pub at: FxVec2,
    /// The battle tick it can first be held.
    pub unlock: u32,
    /// Whether that tick has come.
    pub open: bool,
    /// The party holding it and the tick it took it, while one does.
    pub held: Option<(PlayerId, u32)>,
}

/// Conquest's rulings (see the module docs).
#[derive(Clone, Debug, Default)]
pub(crate) struct Throne {
    victories: Victories,
    /// The Heart's middle and unlock tick, if the map has one and it counts.
    heart: Option<(FxVec2, u32)>,
    held: Option<(PlayerId, u32)>,
    /// Parties that have had a main building: only they can be put out.
    had_main: u8,
    /// Parties put out.
    out: u8,
    /// Parties whose countdown waits on a main building going up.
    paused: u8,
    /// The tick each party lost its last main building, while it has none.
    exiled: [Option<u32>; MAX_PARTIES],
}

impl Throne {
    /// The rulings for a match played for `victories`, with the Heart at
    /// `heart` unlocking at `HEART_UNLOCK_TICKS` scaled by `percent`.
    pub(crate) fn new(victories: Victories, heart: Option<FxVec2>, percent: u32) -> Throne {
        let unlock = (u64::from(HEART_UNLOCK_TICKS) * u64::from(percent) / 100) as u32;
        Throne {
            victories,
            heart: heart.filter(|_| victories.heart).map(|at| (at, unlock)),
            ..Throne::default()
        }
    }

    /// Whether the rulings want to know who has a main building.
    pub(crate) fn counts_mains(&self) -> bool {
        self.victories.annihilation
    }

    /// The Heart `battle` ticks into the battle.
    pub(crate) fn heart(&self, battle: u32) -> Option<HeartView> {
        self.heart.map(|(at, unlock)| HeartView {
            at,
            unlock,
            open: battle >= unlock,
            held: self.held,
        })
    }

    /// The parties with fighting regiments at the Heart, `battle` ticks
    /// into the battle: none before it unlocks.
    fn at_heart(&self, regs: &Regiments, battle: u32) -> u8 {
        let Some((at, unlock)) = self.heart else {
            return 0;
        };
        if battle < unlock {
            return 0;
        }
        let reach = u64::from(HEART_REACH.raw().unsigned_abs()).pow(2);
        (0..regs.alive.len())
            .filter(|&i| fighting(regs, i) && (regs.position[i] - at).length_sq_raw() <= reach)
            .fold(0u8, |m, i| m | 1 << regs.owner[i])
    }

    /// The tick `party` is out unless it lays a main building first, while
    /// it has none and isn't out yet (not while one is going up).
    pub(crate) fn exiled(&self, party: PlayerId) -> Option<u32> {
        if self.paused & (1 << party) != 0 {
            return None;
        }
        let since = self.exiled.get(usize::from(party)).copied().flatten()?;
        Some(since + EXILE_TICKS)
    }

    pub(crate) fn is_out(&self, party: PlayerId) -> bool {
        self.out & (1 << party) != 0
    }

    /// The parties that are out, as a bitmask.
    pub(crate) fn out_mask(&self) -> u8 {
        self.out
    }

    /// One look, at tick `now` (`battle` ticks into the battle), given the
    /// parties with a main building (`mains`). Returns the parties put out
    /// now, and the verdict if this decides the match.
    fn update(
        &mut self,
        regs: &Regiments,
        mains: Mains,
        stances: &Stances,
        now: u32,
        battle: u32,
    ) -> (u8, Option<Verdict>) {
        let Mains { built, rising } = mains;
        let mut put_out = 0;
        if self.victories.annihilation {
            self.had_main |= built | rising;
            self.paused = 0;
            for p in parties(self.had_main & !self.out) {
                let (p, bit) = (usize::from(p), 1u8 << p);
                if built & bit != 0 {
                    self.exiled[p] = None;
                } else if rising & bit != 0 {
                    // Its countdown, if running, waits on the one going up:
                    // calling that off doesn't start it over.
                    if let Some(since) = &mut self.exiled[p] {
                        *since += TICKS_PER_SECOND;
                        self.paused |= bit;
                    }
                } else {
                    let since = *self.exiled[p].get_or_insert(now);
                    if now - since >= EXILE_TICKS {
                        self.exiled[p] = None;
                        put_out |= bit;
                    }
                }
            }
            self.out |= put_out;
        }
        if self.heart.is_some() {
            let at_heart = self.at_heart(regs, battle) & !self.out;
            self.held = match (self.held, one_side(at_heart, stances)) {
                // Allies share a hold, even when the first of them is out.
                (Some((p, since)), Some(h)) if !stances.hostile(p, h) => {
                    Some((if self.is_out(p) { h } else { p }, since))
                }
                (_, Some(h)) => Some((h, now)),
                (_, None) => None,
            };
            if let Some((holder, since)) = self.held
                && now - since >= HEART_HOLD_TICKS
            {
                return (
                    put_out,
                    Some(Verdict {
                        holder,
                        tick: now,
                        by: Win::Heart,
                    }),
                );
            }
        }
        // Once someone is out, the match ends when everyone left is friends:
        // those who have had a main building, and any other with an army.
        if self.out == 0 {
            return (put_out, None);
        }
        let armies = (0..regs.alive.len())
            .filter(|&i| regs.alive[i] && regs.models[i] > 0)
            .fold(0u8, |m, i| m | 1 << regs.owner[i]);
        let left = (self.had_main | armies) & !self.out;
        let verdict = match one_side(left, stances) {
            Some(holder) => Some(Verdict {
                holder,
                tick: now,
                by: Win::Annihilation,
            }),
            // The last of them were put out together.
            None if left == 0 => Some(Verdict {
                holder: put_out.trailing_zeros() as PlayerId,
                tick: now,
                by: Win::Draw,
            }),
            None => None,
        };
        (put_out, verdict)
    }

    fn hash_into(&self, h: &mut StateHasher) {
        match self.held {
            Some((p, since)) => {
                h.u8(1);
                h.u8(p);
                h.u32(since);
            }
            None => h.u8(0),
        }
        h.u8(self.had_main);
        h.u8(self.out);
        h.u8(self.paused);
        for e in self.exiled {
            h.u32(e.unwrap_or(u32::MAX));
        }
    }
}

/// The lowest of the parties in `mask`, if there are any and none of them
/// is another's enemy.
fn one_side(mask: u8, stances: &Stances) -> Option<PlayerId> {
    let first = parties(mask).next()?;
    parties(mask)
        .all(|p| parties(mask).all(|q| !stances.hostile(p, q)))
        .then_some(first)
}

/// The parties in `mask`, lowest first.
fn parties(mask: u8) -> impl Iterator<Item = PlayerId> + Clone {
    (0..MAX_PARTIES as PlayerId).filter(move |&p| mask & (1 << p) != 0)
}

/// Whether regiment `i` is still fighting: not broken, not wiped out.
fn fighting(regs: &Regiments, i: usize) -> bool {
    regs.alive[i] && regs.models[i] > 0 && !regs.morale_state[i].broken()
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Judge {
    /// The party holding the field and the tick it took it, while one does.
    held: Option<(PlayerId, u32)>,
    decided: Option<Verdict>,
    /// Conquest's rulings, which take the place of holding the field.
    throne: Option<Throne>,
}

impl Judge {
    pub(crate) fn verdict(&self) -> Option<Verdict> {
        self.decided
    }

    /// Play the match by Conquest's rulings from now on.
    pub(crate) fn conquest(&mut self, throne: Throne) {
        self.throne = Some(throne);
    }

    pub(crate) fn throne(&self) -> Option<&Throne> {
        self.throne.as_ref()
    }

    /// Conquest's once-a-second look after tick `now` has been played
    /// (`battle` ticks into the battle; see [`Throne::update`]), asking
    /// `mains` for the parties with a main building only if the rulings
    /// need them. Returns the parties put out.
    pub(crate) fn update_conquest(
        &mut self,
        regs: &Regiments,
        mains: impl FnOnce() -> Mains,
        stances: &Stances,
        now: u32,
        battle: u32,
    ) -> u8 {
        let Some(throne) = &mut self.throne else {
            return 0;
        };
        if self.decided.is_some() || !now.is_multiple_of(TICKS_PER_SECOND) {
            return 0;
        }
        let mains = if throne.counts_mains() {
            mains()
        } else {
            Mains::default()
        };
        let (out, verdict) = throne.update(regs, mains, stances, now, battle);
        self.decided = verdict;
        out
    }

    /// Look at the field after tick `now` has been played.
    pub(crate) fn update(
        &mut self,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
        now: u32,
    ) {
        // Once a second is plenty for a ruling that needs ten. Conquest
        // has its own rulings.
        if self.decided.is_some() || self.throne.is_some() || !now.is_multiple_of(TICKS_PER_SECOND)
        {
            return;
        }
        self.held = match (self.held, holding_the_field(regs, stances, vision)) {
            // Allies take turns being the first regiment standing.
            (Some((p, since)), Some(h)) if !stances.hostile(p, h) => Some((p, since)),
            (_, Some(h)) => Some((h, now)),
            (_, None) => None,
        };
        if let Some((holder, since)) = self.held
            && now - since >= HOLD_TICKS
        {
            self.decided = Some(Verdict {
                holder,
                tick: now,
                by: Win::Field,
            });
        }
    }

    /// Nobody holds the field while the armies deploy.
    pub(crate) fn deploying(&mut self) {
        self.held = None;
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        match self.held {
            Some((p, since)) => {
                h.u8(1);
                h.u8(p);
                h.u32(since);
            }
            None => h.u8(0),
        }
        match self.decided {
            Some(v) => {
                h.u8(1);
                h.u8(v.holder);
                h.u32(v.tick);
                h.u8(v.by as u8);
            }
            None => h.u8(0),
        }
        if let Some(t) = &self.throne {
            t.hash_into(h);
        }
    }
}

/// The party holding the field, if one is: it has regiments still fighting
/// (not broken, not wiped out) and sees none of its enemies' fighting
/// regiments. It must also have one not hidden from its enemies, so
/// survivors rallying in a wood don't drag a battle on; two armies out of
/// each other's sight are still facing each other. With no enemy left
/// standing at all, hiding doesn't matter. Among allies that qualify, the
/// lowest-numbered party is named.
pub(crate) fn holding_the_field(
    regs: &Regiments,
    stances: &Stances,
    vision: &Vision,
) -> Option<PlayerId> {
    // Parties with regiments fighting, and those with one in the open.
    let (mut standing, mut shown) = (0u8, 0u8);
    for i in (0..regs.alive.len()).filter(|&i| fighting(regs, i)) {
        let bit = 1u8 << regs.owner[i];
        standing |= bit;
        if !vision.hidden(i, regs.owner[i]) {
            shown |= bit;
        }
    }
    parties(standing).find(|&p| {
        let foes = parties(standing)
            .filter(|&q| stances.hostile(p, q))
            .fold(0u8, |m, q| m | 1 << q);
        if foes == 0 {
            return true;
        }
        shown & (1 << p) != 0
            && !(0..regs.alive.len()).any(|i| {
                fighting(regs, i)
                    && foes & (1 << regs.owner[i]) != 0
                    && vision.sees(p, i, regs.owner[i])
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(j: &Judge) -> u64 {
        let mut h = StateHasher::new();
        j.hash_into(&mut h);
        h.finish()
    }

    #[test]
    fn the_ruling_is_part_of_the_state() {
        let none = Judge::default();
        let held = Judge {
            held: Some((0, 5)),
            ..Judge::default()
        };
        let later = Judge {
            held: Some((0, 6)),
            ..Judge::default()
        };
        let won = Judge {
            held: Some((0, 5)),
            decided: Some(Verdict {
                holder: 0,
                tick: 205,
                by: Win::Field,
            }),
            ..Judge::default()
        };
        let hashes = [hash(&none), hash(&held), hash(&later), hash(&won)];
        for (i, a) in hashes.iter().enumerate() {
            assert!(!hashes[i + 1..].contains(a), "{hashes:x?}");
        }
    }
}
