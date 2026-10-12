//! Who fights whom: the one hostility test every rule asks (docs/18-ai.md,
//! engine hook 1).
//!
//! Parties on the same team are allies: their regiments don't fight each
//! other, can't be ordered to attack each other, and count as friends for
//! morale. Everyone else is hostile. The table is filled from the lobby's
//! teams when the match starts and stays fixed for the match, so like the
//! map it is setup data rather than hashed state. Later it grows into the
//! relations ledger, where stances change during play.

use alloc::vec::Vec;

use crate::PlayerId;

/// Parties' teams, and the hostility test that reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stances {
    /// Team of each party, by party id. A party past the end has no team:
    /// it is hostile to everyone but itself. Empty means every party fights
    /// every other, which is what tests and the sandbox use.
    team: Vec<u8>,
}

impl Stances {
    /// `teams[p]` is party `p`'s team; parties with the same number are allies.
    pub fn from_teams(teams: &[u8]) -> Stances {
        Stances {
            team: teams.to_vec(),
        }
    }

    /// Whether `a`'s regiments fight `b`'s.
    pub fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        if a == b {
            return false;
        }
        match (self.team.get(a as usize), self.team.get(b as usize)) {
            (Some(x), Some(y)) => x != y,
            _ => true,
        }
    }

    /// Team of each party, as given at the start of the match.
    pub fn teams(&self) -> &[u8] {
        &self.team
    }

    /// Bit `s` is set when a party whose id is `s` modulo `slots` is an ally
    /// of `party` (or `party` itself). The combat grid files regiments by
    /// `owner % slots`, so a search for friends only visits these.
    pub(crate) fn friend_slots(&self, party: PlayerId, slots: usize) -> u32 {
        let mut mask = 1 << (party as usize % slots);
        if let Some(&mine) = self.team.get(party as usize) {
            for (p, &t) in self.team.iter().enumerate() {
                if t == mine {
                    mask |= 1 << (p % slots);
                }
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_teams_everyone_fights_everyone_else() {
        let s = Stances::default();
        assert!(!s.hostile(2, 2));
        assert!(s.hostile(0, 1));
        assert!(s.hostile(7, 200));
        assert_eq!(s.friend_slots(3, 8), 1 << 3);
    }

    #[test]
    fn teammates_are_allies() {
        // Two against two: players 0 and 2 against 1 and 3.
        let s = Stances::from_teams(&[0, 1, 0, 1]);
        assert!(!s.hostile(0, 2));
        assert!(!s.hostile(3, 1));
        assert!(s.hostile(0, 1));
        assert!(s.hostile(2, 3));
        // A party the lobby didn't list is on its own.
        assert!(s.hostile(0, 9));
        assert!(s.hostile(9, 0));
        assert_eq!(s.friend_slots(0, 8), 0b0101);
        assert_eq!(s.friend_slots(9, 8), 1 << 1);
    }
}
