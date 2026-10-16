//! Who won: one ruling, made by the simulation itself every tick, so the
//! game, the golden replays, the computer players' test harnesses and every
//! machine in a network match agree on it (review fix R5).
//!
//! A side wins by **holding the field**: it still has regiments fighting,
//! and none of its enemies' fighting regiments are in its sight, for
//! [`HOLD_TICKS`] in a row (looked at once a second). Allies share the hold. Once decided, the
//! verdict stands for the rest of the match; the battle itself goes on.

use crate::hash::StateHasher;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::vision::Vision;
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// How long a side must hold the field to win.
pub const HOLD_TICKS: u32 = 10 * TICKS_PER_SECOND;

/// How a battle was decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The party holding the field when it was decided (its allies won too).
    pub holder: PlayerId,
    /// The tick it was decided on, counted from the start of the match.
    pub tick: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Judge {
    /// The party holding the field and the tick it took it, while one does.
    held: Option<(PlayerId, u32)>,
    decided: Option<Verdict>,
}

impl Judge {
    pub(crate) fn verdict(&self) -> Option<Verdict> {
        self.decided
    }

    /// Look at the field after tick `now` has been played.
    pub(crate) fn update(
        &mut self,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
        now: u32,
    ) {
        // Once a second is plenty for a ruling that needs ten.
        if self.decided.is_some() || !now.is_multiple_of(TICKS_PER_SECOND) {
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
            self.decided = Some(Verdict { holder, tick: now });
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
            }
            None => h.u8(0),
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
    let fighting = |i: usize| regs.alive[i] && regs.models[i] > 0 && !regs.morale_state[i].broken();
    for i in (0..regs.alive.len()).filter(|&i| fighting(i)) {
        let bit = 1u8 << regs.owner[i];
        standing |= bit;
        if !vision.hidden(i, regs.owner[i]) {
            shown |= bit;
        }
    }
    (0..MAX_PARTIES as PlayerId).find(|&p| {
        if standing & (1 << p) == 0 {
            return false;
        }
        let foes = (0..MAX_PARTIES as PlayerId)
            .filter(|&q| standing & (1 << q) != 0 && stances.hostile(p, q))
            .fold(0u8, |m, q| m | 1 << q);
        if foes == 0 {
            return true;
        }
        shown & (1 << p) != 0
            && !(0..regs.alive.len()).any(|i| {
                fighting(i) && foes & (1 << regs.owner[i]) != 0 && vision.sees(p, i, regs.owner[i])
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
            decided: None,
        };
        let later = Judge {
            held: Some((0, 6)),
            decided: None,
        };
        let won = Judge {
            held: Some((0, 5)),
            decided: Some(Verdict {
                holder: 0,
                tick: 205,
            }),
        };
        let hashes = [hash(&none), hash(&held), hash(&later), hash(&won)];
        for (i, a) in hashes.iter().enumerate() {
            assert!(!hashes[i + 1..].contains(a), "{hashes:x?}");
        }
    }
}
